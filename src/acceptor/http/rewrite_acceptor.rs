// MIT License

// Copyright (c) 2019 Zhuhao Wang

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use crate::{
    acceptor::Acceptor,
    core::{Endpoint, Result},
    utils::OwningMutexGuard,
};
use async_trait::async_trait;
use futures::{
    channel::oneshot,
    future::{self, BoxFuture, Either, FutureExt, TryFutureExt},
    io::{AsyncRead, AsyncWrite},
    lock::Mutex,
};
use futures_tokio_compat::Compat;
use http::uri::Scheme;
use hyper::{
    client::conn::{Builder, SendRequest},
    http::{Request, Response},
    server::conn::Http,
    service, Body,
};
use std::sync::{Arc, Mutex as StdMutex};

pub enum RewriteResult {
    Request(Request<Body>),
    Response(Response<Body>),
}

pub trait Rewriter {
    fn handle(&mut self, req: Request<Body>) -> RewriteResult;
}

#[derive(new)]
pub struct HttpRewriteAcceptor<
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    R: Rewriter + Send + 'static,
> {
    io: T,
    rewriter: R,
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static, R: Rewriter + Send + 'static>
    Acceptor<Option<HttpRewriteAccpetorMidHandshake>> for HttpRewriteAcceptor<T, R>
{
    async fn handshake(mut self) -> Result<Option<HttpRewriteAccpetorMidHandshake>> {
        let request_handler: Arc<Mutex<Option<SendRequest<Body>>>> = Arc::new(Mutex::new(None));
        let handler = request_handler.clone();
        let (sender, receiver) = oneshot::channel::<Endpoint>();
        let sender = StdMutex::new(Some(sender));
        let mut rewriter = self.rewriter;
        let connection = Http::new().serve_connection(
            Compat::new(self.io),
            service::service_fn(move |req: Request<Body>| {
                let result = rewriter.handle(req);
                let handler = request_handler.clone();

                if let RewriteResult::Request(req) = &result {
                    match sender.try_lock() {
                        Ok(mut m) => match m.take() {
                            Some(s) => {
                                if let Some(host) = req.uri().host().map(|h| h.to_string()) {
                                    let mut port = 80;
                                    if let Some(p) = req.uri().port_part() {
                                        port = p.as_u16();
                                    } else if req.uri().scheme_part() == Some(&Scheme::HTTPS) {
                                        port = 443;
                                    }
                                    s.send(Endpoint::HostName(host, port)).unwrap();
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    };
                }

                async move {
                    match result {
                        RewriteResult::Request(req) => {
                            handler
                                .lock()
                                .await
                                .as_mut()
                                .unwrap()
                                .send_request(req)
                                .await
                        }
                        RewriteResult::Response(res) => Ok(res),
                    }
                }
            }),
        );

        let mutex_guard = OwningMutexGuard::new(handler, |h| h.try_lock().unwrap());
        match future::try_select(receiver, connection).await {
            Ok(Either::Left((endpoint, connection))) => Ok(Some(HttpRewriteAccpetorMidHandshake {
                connection: connection.err_into().boxed(),
                endpoint,
                mutex_guard,
            })),
            Ok(Either::Right((_, _))) => Ok(None),
            Err(Either::Right((err, _))) => Err(err.into()),
            _ => unreachable!(),
        }
    }
}

pub struct HttpRewriteAccpetorMidHandshake {
    connection: BoxFuture<'static, Result<()>>,
    endpoint: Endpoint,
    mutex_guard: OwningMutexGuard<Option<SendRequest<Body>>>,
}

impl HttpRewriteAccpetorMidHandshake {
    pub fn target_endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub async fn finalize<T: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        mut self,
        io: T,
    ) -> Result<()> {
        let (send_request, remote_connection) = Builder::new().handshake(Compat::new(io)).await?;
        *self.mutex_guard = Some(send_request);
        match future::try_select(self.connection, remote_connection).await {
            Ok(Either::Left(_)) => Ok(()),
            Ok(Either::Right((_, local_connection))) => local_connection.await,
            Err(Either::Left((err, _))) => Err(err.into()),
            Err(Either::Right((err, _))) => Err(err.into()),
        }
    }
}
