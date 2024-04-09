use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};

use futures_lite::{AsyncRead, Future, FutureExt};
use isahc::{AsyncBody, ResponseFuture};

use crate::{prelude::Request, DynResult};

use super::{response::UnixResponse, SHARED};

#[derive(Clone, Copy)]
enum RequestState {
    Building,
    Recv,
}

pub struct UnixRequest {
    state: RequestState,
    req_builder: Option<isahc::http::request::Builder>,
    body: Option<Box<dyn AsyncRead + Unpin + Send + Sync + 'static>>,
    res: Option<ResponseFuture<'static>>,
}

impl UnixRequest {
    pub(crate) fn new(req_builder: isahc::http::request::Builder) -> Self {
        Self {
            state: RequestState::Building,
            req_builder: Some(req_builder),
            body: None,
            res: None,
        }
    }
}

impl Future for UnixRequest {
    type Output = DynResult<UnixResponse>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state {
            RequestState::Building => {
                if let Some(req_builder) = self.req_builder.take() {
                    let body = self
                        .body
                        .take()
                        .unwrap_or_else(|| Box::new(futures_lite::io::empty()));
                    match req_builder.body(AsyncBody::from_reader(body)) {
                        Ok(req) => {
                            let res = SHARED.send_async(req);
                            self.res = Some(res);
                            self.state = RequestState::Recv;
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                        Err(_) => Poll::Ready(Err({
                            #[cfg(not(feature = "anyhow"))]
                            {
                                Box::from("isahc error")
                            }
                            #[cfg(feature = "anyhow")]
                            {
                                anyhow::anyhow!("isahc error")
                            }
                        })),
                    }
                } else {
                    Poll::Ready(Err({
                        #[cfg(not(feature = "anyhow"))]
                        {
                            Box::from("already polled")
                        }
                        #[cfg(feature = "anyhow")]
                        {
                            anyhow::anyhow!("already polled")
                        }
                    }))
                }
            }
            RequestState::Recv => {
                if let Some(res) = &mut self.as_mut().res {
                    match res.poll(cx) {
                        Poll::Ready(Ok(res)) => {
                            let code = res.status().as_u16();
                            let mut headers = HashMap::with_capacity(res.headers().len());
                            for (name, value) in res.headers().iter() {
                                headers.insert(
                                    name.as_str().to_string(),
                                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                                );
                            }
                            Poll::Ready(Ok(UnixResponse {
                                res: res.into_body(),
                                code,
                                headers,
                            }))
                        }
                        Poll::Ready(Err(_)) => Poll::Ready(Err({
                            #[cfg(not(feature = "anyhow"))]
                            {
                                Box::from("isahc error")
                            }
                            #[cfg(feature = "anyhow")]
                            {
                                anyhow::anyhow!("isahc error")
                            }
                        })),
                        Poll::Pending => Poll::Pending,
                    }
                } else {
                    Poll::Ready(Err({
                        #[cfg(not(feature = "anyhow"))]
                        {
                            Box::from("already polled")
                        }
                        #[cfg(feature = "anyhow")]
                        {
                            anyhow::anyhow!("already polled")
                        }
                    }))
                }
            }
        }
    }
}

impl Request for UnixRequest {
    fn body(
        mut self,
        new_body: impl AsyncRead + Unpin + Send + Sync + 'static,
        _body_size: usize,
    ) -> Self {
        self.body = Some(Box::new(new_body));
        self
    }

    fn header(mut self, header: &str, value: &str) -> Self {
        let req_builder = self.req_builder.take();
        if let Some(req_builder) = req_builder {
            self.req_builder = Some(req_builder.header(header, value));
        }
        self
    }
}
