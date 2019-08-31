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
    acceptor::{http::HttpError, Acceptor},
    core::{Endpoint, Error, Result},
};
use async_trait::async_trait;
use futures::{
    channel::oneshot,
    future::{self, Either, TryFutureExt},
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
};
use futures_tokio_compat::Compat;
use hyper::{
    http::{method::Method, Request, Response},
    server::conn::Http,
    service, Body,
};
use std::sync::{Arc, Mutex};

pub struct HttpConnectAcceptor<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    io: T,
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static>
    Acceptor<HttpConnectMidHandshake<Compat<Compat<T>>>> for HttpConnectAcceptor<T>
{
    async fn handshake(self) -> Result<HttpConnectMidHandshake<Compat<Compat<T>>>> {
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let connection = Http::new().serve_connection(
            Compat::new(self.io),
            service::service_fn(move |req: Request<Body>| {
                let sender = sender.clone();
                async move {
                    match *req.method() {
                        Method::CONNECT => {
                            if let (Some(host), Some(port)) = (
                                req.uri().host().map(|h| h.to_string()),
                                req.uri().port_part().map(|p| p.as_u16()),
                            ) {
                                sender
                                    .lock()
                                    .unwrap()
                                    .take()
                                    .unwrap()
                                    .send(Endpoint::HostName(host, port))
                                    .unwrap();
                                Ok::<Response<Body>, _>(future::pending().await)
                            } else {
                                Err(HttpError::InvalidConnectUrl(req.uri().to_string()))
                            }
                        }
                        _ => Err(HttpError::InvalidCommand(req.method().to_string())),
                    }
                }
            }),
        );

        let res = future::try_select(receiver, connection).await;
        match res {
            Ok(Either::Left((endpoint, connection))) => Ok(HttpConnectMidHandshake {
                io: Compat::new(connection.into_parts().io),
                endpoint,
            }),
            Ok(Either::Right((_, _))) => Err(HttpError::ClosedWithoutRequest.into()),
            Err(Either::Right((err, _))) => Err(err.into()),
            _ => unreachable!(),
        }
    }
}

pub struct HttpConnectMidHandshake<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    io: T,
    endpoint: Endpoint,
}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> HttpConnectMidHandshake<T> {
    pub fn target_endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub async fn finalize(mut self) -> Result<T> {
        let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
        self.io
            .write_all(response.as_bytes())
            .err_into::<Error>()
            .await?;
        Ok(self.io)
    }
}
