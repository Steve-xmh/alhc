#![cfg(unix)]

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
    time::Duration,
};

use alhc::prelude::*;
use futures::future::join_all;
use pollster::FutureExt;

#[test]
fn multi_driver_handles_concurrent_requests() {
    const REQUESTS: usize = 32;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut handlers = Vec::with_capacity(REQUESTS);
        for _ in 0..REQUESTS {
            let (mut stream, _) = listener.accept().unwrap();
            handlers.push(thread::spawn(move || {
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = [0; 2048];
                let length = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                assert!(request
                    .lines()
                    .any(|line| line.eq_ignore_ascii_case("x-alhc-test: multi")));

                thread::sleep(Duration::from_millis(20));
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-Multi: yes\r\nConnection: close\r\n\r\nok",
                    )
                    .unwrap();
            }));
        }
        for handler in handlers {
            handler.join().unwrap();
        }
    });

    let client = Arc::new(get_client_builder().build().unwrap());
    let url = format!("http://{address}/test");
    let responses = async {
        join_all((0..REQUESTS).map(|_| {
            let client = client.clone();
            let url = url.clone();
            async move {
                client
                    .get(&url)
                    .unwrap()
                    .header("X-Alhc-Test", "multi")
                    .await
                    .unwrap()
                    .recv()
                    .await
                    .unwrap()
            }
        }))
        .await
    }
    .block_on();

    for response in responses {
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.header("X-Multi"), Some("yes"));
        assert_eq!(response.data(), b"ok");
    }
    server.join().unwrap();
}
