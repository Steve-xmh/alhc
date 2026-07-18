use std::{
    collections::VecDeque,
    ffi::{CString, c_char, c_void},
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{Arc, Mutex, MutexGuard},
    task::Waker,
};

use crate::response::HeaderMap;

use super::{CurlResponse, curl_bind};

pub(crate) type RequestResult = std::io::Result<CurlResponse>;

pub(crate) struct RequestState {
    pub(crate) result: Option<RequestResult>,
    pub(crate) waker: Option<Waker>,
}

impl RequestState {
    pub(crate) fn new() -> Self {
        Self {
            result: None,
            waker: None,
        }
    }
}

pub(crate) struct RequestConfig {
    pub(crate) url: CString,
    pub(crate) method: CString,
    pub(crate) headers: Vec<CString>,
    pub(crate) body: Vec<u8>,
    pub(crate) state: Arc<Mutex<RequestState>>,
}

#[derive(Clone, Copy)]
pub(crate) struct RequestToken {
    slot: usize,
    generation: usize,
}

enum Command {
    Start {
        token: RequestToken,
        config: RequestConfig,
    },
    Cancel(RequestToken),
    Shutdown,
}

struct SlotAllocation {
    generation: usize,
    used: bool,
}

struct QueueState {
    commands: VecDeque<Command>,
    slots: Vec<SlotAllocation>,
    free_slots: Vec<usize>,
    stopped: bool,
}

pub(crate) struct CurlDriver {
    queue: Arc<Mutex<QueueState>>,
    wake_writer: UnixStream,
}

impl std::fmt::Debug for CurlDriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CurlDriver")
    }
}

impl CurlDriver {
    pub(crate) fn new() -> std::io::Result<Arc<Self>> {
        ensure_global_init()?;

        let (wake_reader, wake_writer) = UnixStream::pair()?;
        let queue = Arc::new(Mutex::new(QueueState {
            commands: VecDeque::new(),
            slots: Vec::new(),
            free_slots: Vec::new(),
            stopped: false,
        }));
        let worker_queue = queue.clone();

        std::thread::Builder::new()
            .name("alhc-curl-multi".into())
            .spawn(move || run_driver(worker_queue, wake_reader))?;

        let mut status = [0];
        (&wake_writer).read_exact(&mut status).map_err(|_| {
            std::io::Error::other("libcurl multi driver stopped during initialization")
        })?;
        if status[0] != 1 {
            return Err(std::io::Error::other("curl_multi_init returned NULL"));
        }
        wake_writer.set_nonblocking(true)?;

        Ok(Arc::new(Self { queue, wake_writer }))
    }

    pub(crate) fn submit(&self, config: RequestConfig) -> std::io::Result<RequestToken> {
        let token = {
            let mut queue = lock_queue(&self.queue);
            if queue.stopped {
                return Err(std::io::Error::other("libcurl multi driver has stopped"));
            }

            let token = allocate_slot(&mut queue);
            queue.commands.push_back(Command::Start { token, config });
            token
        };
        self.wake();
        Ok(token)
    }

    pub(crate) fn cancel(&self, token: RequestToken) {
        let queued = {
            let mut queue = lock_queue(&self.queue);
            if queue.stopped {
                false
            } else {
                queue.commands.push_back(Command::Cancel(token));
                true
            }
        };
        if queued {
            self.wake();
        }
    }

    fn wake(&self) {
        match (&self.wake_writer).write(&[1]) {
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => {}
        }
    }
}

impl Drop for CurlDriver {
    fn drop(&mut self) {
        {
            let mut queue = lock_queue(&self.queue);
            if !queue.stopped {
                queue.commands.push_back(Command::Shutdown);
            }
        }
        self.wake();
    }
}

fn ensure_global_init() -> std::io::Result<()> {
    static RESULT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    RESULT
        .get_or_init(|| curl_bind::global_init().map_err(|error| error.to_string()))
        .clone()
        .map_err(std::io::Error::other)
}

fn lock_queue(queue: &Mutex<QueueState>) -> MutexGuard<'_, QueueState> {
    queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn allocate_slot(queue: &mut QueueState) -> RequestToken {
    if let Some(slot) = queue.free_slots.pop() {
        let allocation = &mut queue.slots[slot];
        allocation.used = true;
        allocation.generation = allocation.generation.wrapping_add(1).max(1);
        RequestToken {
            slot,
            generation: allocation.generation,
        }
    } else {
        let slot = queue.slots.len();
        queue.slots.push(SlotAllocation {
            generation: 1,
            used: true,
        });
        RequestToken {
            slot,
            generation: 1,
        }
    }
}

fn release_slot(queue: &Mutex<QueueState>, token: RequestToken) {
    let mut queue = lock_queue(queue);
    if let Some(allocation) = queue.slots.get_mut(token.slot) {
        if allocation.used && allocation.generation == token.generation {
            allocation.used = false;
            queue.free_slots.push(token.slot);
        }
    }
}

struct Transfer {
    token: RequestToken,
    easy: curl_bind::EasyHandle,
    header_list: curl_bind::SlistHandle,
    _url: CString,
    _method: CString,
    _request_headers: Vec<CString>,
    _request_body: Vec<u8>,
    response_body: Vec<u8>,
    raw_headers: Vec<u8>,
    callback_failed: bool,
    state: Arc<Mutex<RequestState>>,
}

unsafe extern "C" fn write_callback(
    pointer: *mut c_char,
    size: usize,
    count: usize,
    userdata: *mut c_void,
) -> usize {
    append_callback_data(pointer, size, count, userdata, false)
}

unsafe extern "C" fn header_callback(
    pointer: *mut c_char,
    size: usize,
    count: usize,
    userdata: *mut c_void,
) -> usize {
    append_callback_data(pointer, size, count, userdata, true)
}

unsafe fn append_callback_data(
    pointer: *mut c_char,
    size: usize,
    count: usize,
    userdata: *mut c_void,
    is_header: bool,
) -> usize {
    let Some(length) = size.checked_mul(count) else {
        return 0;
    };
    if length == 0 || pointer.is_null() || userdata.is_null() {
        return 0;
    }

    let transfer = &mut *(userdata as *mut Transfer);
    let target = if is_header {
        &mut transfer.raw_headers
    } else {
        &mut transfer.response_body
    };
    if target.try_reserve(length).is_err() {
        transfer.callback_failed = true;
        return 0;
    }

    let data = std::slice::from_raw_parts(pointer as *const u8, length);
    target.extend_from_slice(data);
    length
}

fn run_driver(queue: Arc<Mutex<QueueState>>, mut wake_reader: UnixStream) {
    let multi = curl_bind::multi_init();
    if multi.is_null() || wake_reader.set_nonblocking(true).is_err() {
        lock_queue(&queue).stopped = true;
        let _ = wake_reader.write_all(&[0]);
        if !multi.is_null() {
            unsafe {
                let _ = curl_bind::multi_cleanup(multi);
            }
        }
        return;
    }
    if wake_reader.write_all(&[1]).is_err() {
        lock_queue(&queue).stopped = true;
        unsafe {
            let _ = curl_bind::multi_cleanup(multi);
        }
        return;
    }

    let mut transfers: Vec<Option<Box<Transfer>>> = Vec::new();
    let mut shutdown = false;

    while !shutdown {
        drain_wakeup(&mut wake_reader);
        let mut commands = {
            let mut queue = lock_queue(&queue);
            std::mem::take(&mut queue.commands)
        };

        while let Some(command) = commands.pop_front() {
            match command {
                Command::Start { token, config } if !shutdown => {
                    if let Some(transfer) = start_transfer(multi, token, config, &queue) {
                        if transfers.len() <= token.slot {
                            transfers.resize_with(token.slot + 1, || None);
                        }
                        transfers[token.slot] = Some(transfer);
                    }
                }
                Command::Start { token, config } => {
                    release_slot(&queue, token);
                    complete_state(
                        &config.state,
                        Err(std::io::Error::other("libcurl multi driver stopped")),
                    );
                }
                Command::Cancel(token) => {
                    cancel_transfer(multi, token, &mut transfers, &queue);
                }
                Command::Shutdown => shutdown = true,
            }
        }

        if shutdown {
            break;
        }

        if curl_bind::multi_perform(multi).is_err() {
            break;
        }

        while let Some((easy, result_code, reported_slot)) = curl_bind::multi_next_done(multi) {
            let slot = matching_slot(&transfers, easy, reported_slot);
            let Some(slot) = slot else {
                continue;
            };
            let Some(transfer) = transfers[slot].take() else {
                continue;
            };
            finish_transfer(multi, transfer, result_code, &queue);
        }

        let timeout = curl_bind::multi_timeout_ms(multi).unwrap_or(1000);
        if curl_bind::multi_wait_fd(multi, wake_reader.as_raw_fd(), timeout).is_err() {
            break;
        }
    }

    fail_all(
        multi,
        &mut transfers,
        &queue,
        "libcurl multi driver stopped",
    );
    fail_queued(&queue, "libcurl multi driver stopped");
    unsafe {
        let _ = curl_bind::multi_cleanup(multi);
    }
}

fn matching_slot(
    transfers: &[Option<Box<Transfer>>],
    easy: curl_bind::EasyHandle,
    reported_slot: usize,
) -> Option<usize> {
    if transfers
        .get(reported_slot)
        .and_then(Option::as_ref)
        .is_some_and(|transfer| transfer.easy == easy)
    {
        Some(reported_slot)
    } else {
        transfers
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|transfer| transfer.easy == easy))
    }
}

fn start_transfer(
    multi: curl_bind::MultiHandle,
    token: RequestToken,
    config: RequestConfig,
    queue: &Mutex<QueueState>,
) -> Option<Box<Transfer>> {
    let easy = curl_bind::easy_init();
    if easy.is_null() {
        release_slot(queue, token);
        complete_state(
            &config.state,
            Err(std::io::Error::other("curl_easy_init returned NULL")),
        );
        return None;
    }

    let mut transfer = Box::new(Transfer {
        token,
        easy,
        header_list: std::ptr::null_mut(),
        _url: config.url,
        _method: config.method,
        _request_headers: config.headers,
        _request_body: config.body,
        response_body: Vec::new(),
        raw_headers: Vec::new(),
        callback_failed: false,
        state: config.state,
    });
    let userdata = transfer.as_mut() as *mut Transfer as *mut c_void;

    let configured = curl_bind::easy_configure(
        easy,
        &transfer._url,
        &transfer._method,
        userdata,
        token.slot,
        write_callback,
        header_callback,
    )
    .and_then(|_| {
        if transfer._request_body.is_empty() {
            Ok(())
        } else {
            curl_bind::easy_set_body(easy, &transfer._request_body)
        }
    })
    .and_then(|_| {
        curl_bind::easy_set_headers(easy, &transfer._request_headers).map(|list| {
            transfer.header_list = list;
        })
    });

    if let Err(error) = configured {
        let state = transfer.state.clone();
        let message = error.to_string();
        destroy_transfer(multi, transfer, false);
        release_slot(queue, token);
        complete_state(&state, Err(std::io::Error::other(message)));
        return None;
    }

    if let Err(error) = curl_bind::multi_add(multi, easy) {
        let state = transfer.state.clone();
        let message = error.to_string();
        destroy_transfer(multi, transfer, false);
        release_slot(queue, token);
        complete_state(&state, Err(std::io::Error::other(message)));
        return None;
    }

    Some(transfer)
}

fn finish_transfer(
    multi: curl_bind::MultiHandle,
    mut transfer: Box<Transfer>,
    result_code: i32,
    queue: &Mutex<QueueState>,
) {
    let _ = curl_bind::multi_remove(multi, transfer.easy);

    let result = if transfer.callback_failed {
        Err(std::io::Error::other(
            "failed to buffer data received from libcurl",
        ))
    } else if result_code != 0 {
        Err(std::io::Error::other(
            curl_bind::CurlError(result_code).to_string(),
        ))
    } else {
        match curl_bind::getinfo_response_code(transfer.easy) {
            Ok(code) if (0..=u16::MAX as i64).contains(&code) => Ok(CurlResponse {
                data: std::mem::take(&mut transfer.response_body),
                position: 0,
                code: code as u16,
                headers: parse_headers(&transfer.raw_headers),
            }),
            Ok(code) => Err(std::io::Error::other(format!(
                "invalid HTTP response status code: {code}"
            ))),
            Err(error) => Err(std::io::Error::other(error.to_string())),
        }
    };

    let token = transfer.token;
    let state = transfer.state.clone();
    unsafe {
        curl_bind::slist_free(transfer.header_list);
        curl_bind::easy_cleanup(transfer.easy);
    }
    drop(transfer);
    release_slot(queue, token);
    complete_state(&state, result);
}

fn cancel_transfer(
    multi: curl_bind::MultiHandle,
    token: RequestToken,
    transfers: &mut [Option<Box<Transfer>>],
    queue: &Mutex<QueueState>,
) {
    let Some(entry) = transfers.get_mut(token.slot) else {
        return;
    };
    if !entry
        .as_ref()
        .is_some_and(|transfer| transfer.token.generation == token.generation)
    {
        return;
    }
    if let Some(transfer) = entry.take() {
        destroy_transfer(multi, transfer, true);
        release_slot(queue, token);
    }
}

// The box keeps callback userdata at a stable address until after the easy
// handle has been removed and cleaned up.
#[allow(clippy::boxed_local)]
fn destroy_transfer(
    multi: curl_bind::MultiHandle,
    transfer: Box<Transfer>,
    remove_from_multi: bool,
) {
    if remove_from_multi {
        let _ = curl_bind::multi_remove(multi, transfer.easy);
    }
    unsafe {
        curl_bind::slist_free(transfer.header_list);
        curl_bind::easy_cleanup(transfer.easy);
    }
    drop(transfer);
}

fn fail_all(
    multi: curl_bind::MultiHandle,
    transfers: &mut [Option<Box<Transfer>>],
    queue: &Mutex<QueueState>,
    message: &str,
) {
    for entry in transfers {
        if let Some(transfer) = entry.take() {
            let token = transfer.token;
            let state = transfer.state.clone();
            destroy_transfer(multi, transfer, true);
            release_slot(queue, token);
            complete_state(&state, Err(std::io::Error::other(message.to_owned())));
        }
    }
}

fn fail_queued(queue: &Mutex<QueueState>, message: &str) {
    let commands = {
        let mut queue = lock_queue(queue);
        queue.stopped = true;
        std::mem::take(&mut queue.commands)
    };

    for command in commands {
        if let Command::Start { token, config } = command {
            release_slot(queue, token);
            complete_state(
                &config.state,
                Err(std::io::Error::other(message.to_owned())),
            );
        }
    }
}

fn complete_state(state: &Arc<Mutex<RequestState>>, result: RequestResult) {
    let waker = {
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.result.is_some() {
            return;
        }
        state.result = Some(result);
        state.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

fn drain_wakeup(reader: &mut UnixStream) {
    let mut buffer = [0; 64];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
}

fn parse_headers(raw: &[u8]) -> HeaderMap {
    let text = String::from_utf8_lossy(raw);
    let mut current = HeaderMap::new();
    let mut final_headers = HeaderMap::new();

    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("HTTP/") {
            current.clear();
        } else if line.is_empty() {
            if !current.is_empty() {
                final_headers = std::mem::take(&mut current);
            }
        } else if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if let Some((_, existing)) = current
                .iter_mut()
                .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            {
                existing.push_str("; ");
                existing.push_str(value);
            } else {
                current.push((name.to_owned(), value.to_owned()));
            }
        }
    }

    if current.is_empty() {
        final_headers
    } else {
        current
    }
}
