// MIT License

// Copyright (c) 2018 Zhuhao Wang

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

use super::{Client, ClientBuilder};
use crate::core::{Endpoint, Error};
use crate::io::forward;
use futures::{future::Either, sync::oneshot};
use http::uri::Scheme;
use hyper::{
    body::Body,
    client::conn as client_conn,
    server::conn::{Connection as ServerConnection, Http},
    service::Service,
    Method, Request, Response,
};
use std::sync::{Arc, Mutex};
use tokio::{io, prelude::*};

#[derive(Debug)]
enum HttpError {
    InvalidConnectUrl,
    InvalidUrl,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{:#?}", self)
    }
}

impl std::error::Error for HttpError {}

pub struct HttpAcceptor<Next: AsyncRead + AsyncWrite + Send + 'static> {
    next: Next,
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static> HttpAcceptor<Next> {
    pub fn new(next: Next) -> Self {
        HttpAcceptor { next }
    }

    pub fn handshake(self) -> Box<Future<Item = MidHandshake<Next>, Error = Error> + Send> {
        let (req_sender, req_receiver) = oneshot::channel();
        let (res_sender, res_receiver) = oneshot::channel();

        let client: Arc<Mutex<Option<Box<dyn Client + Send>>>> = Arc::new(Mutex::new(None));
        let service = HttpService::new(req_sender, res_receiver, client.clone());
        let connection = Http::new().serve_connection(self.next, service);
        let fill_session =
            req_receiver
                .map_err(|_| panic!())
                .and_then(move |req| match *req.method() {
                    Method::CONNECT => {
                        if let (Some(host), Some(port)) = (
                            req.uri().host().map(|h| h.to_string()),
                            req.uri().port_part().map(|p| p.as_u16()),
                        ) {
                            future::ok(MidHandshake {
                                res_channel: res_sender.into(),
                                is_connect: true,
                                connection: None,
                                first_req: req,
                                client,
                                target_endpoint: Endpoint::HostName(host, port),
                            })
                        } else {
                            future::err(HttpError::InvalidConnectUrl)
                        }
                    }
                    _ => {
                        if let Some(host) = req.uri().host().map(|h| h.to_string()) {
                            let mut port = 80;
                            if let Some(p) = req.uri().port_part() {
                                port = p.as_u16();
                            } else if req.uri().scheme_part() == Some(&Scheme::HTTPS) {
                                port = 443;
                            }

                            future::ok(MidHandshake {
                                res_channel: res_sender.into(),
                                is_connect: false,
                                connection: None,
                                first_req: req,
                                client,
                                target_endpoint: Endpoint::HostName(host, port),
                            })
                        } else {
                            future::err(HttpError::InvalidUrl)
                        }
                    }
                });
        Box::new(
            fill_session
                .select2(connection)
                .map_err(|e| match e {
                    Either::A((err, _)) => Box::new(err) as Error,
                    Either::B((err, _)) => Box::new(err),
                })
                .and_then(|r| match r {
                    Either::A((mut handshake, connection)) => {
                        handshake.connection = Some(connection);
                        future::ok(handshake)
                    }
                    Either::B(_) => future::err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Connection closed without any request.",
                    )) as Error),
                }),
        )
    }
}

type ResponseSender = oneshot::Sender<Box<Future<Item = Response<Body>, Error = Error> + Send>>;
pub struct MidHandshake<Next: AsyncRead + AsyncWrite + Send + 'static> {
    res_channel: Option<ResponseSender>,
    is_connect: bool,
    connection: Option<ServerConnection<Next, HttpService>>,
    first_req: Request<Body>,
    client: Arc<Mutex<Option<Box<dyn Client + Send>>>>,
    target_endpoint: Endpoint,
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static> MidHandshake<Next> {
    pub fn target_endpoint(&self) -> &Endpoint {
        &self.target_endpoint
    }

    pub fn complete_with<
        Io: AsyncRead + AsyncWrite + Send + 'static,
        B: ClientBuilder + Send + 'static,
    >(
        self,
        io: Io,
        client_builder: B,
    ) -> Box<Future<Item = (), Error = Error> + Send> {
        if self.is_connect {
            Box::new(
                io::write_all(
                    self.connection.unwrap().into_parts().io,
                    "HTTP/1.1 200 Connection Established\r\n\r\n",
                )
                .and_then(move |(next, _)| forward(next, io))
                .map(|_| {})
                .from_err(),
            )
        } else {
            Box::new(client_conn::handshake(io).from_err().and_then(
                move |(handler, connection)| {
                    let mut client = client_builder.build(handler);
                    assert!(self
                        .res_channel
                        .unwrap()
                        .send(Box::new(client.send_request(self.first_req).from_err()))
                        .is_ok());
                    *self.client.lock().unwrap() = Some(client);

                    self.connection
                        .unwrap()
                        .select2(connection)
                        .map_err(|e| match e {
                            Either::A((err, _)) => Box::new(err) as Error,
                            Either::B((err, _)) => Box::new(err),
                        })
                        .and_then(|r| match r {
                            Either::A(_) => Box::new(future::ok(()))
                                as Box<Future<Item = (), Error = Error> + Send>,
                            Either::B((_, conn)) => Box::new(conn.from_err()),
                        })
                },
            ))
        }
    }
}

type ResponseReciever =
    oneshot::Receiver<Box<dyn Future<Item = Response<Body>, Error = Error> + Send>>;

struct HttpService {
    client: Arc<Mutex<Option<Box<dyn Client + Send>>>>,
    req_sender: Option<oneshot::Sender<Request<Body>>>,
    res_receiver: Option<ResponseReciever>,
}

impl HttpService {
    fn new(
        req_sender: oneshot::Sender<Request<Body>>,
        res_receiver: ResponseReciever,
        client: Arc<Mutex<Option<Box<dyn Client + Send>>>>,
    ) -> Self {
        HttpService {
            client,
            req_sender: req_sender.into(),
            res_receiver: res_receiver.into(),
        }
    }
}

impl Service for HttpService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = Error;
    type Future = Box<Future<Item = Response<Self::ResBody>, Error = Self::Error> + Send>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        if self.client.lock().unwrap().is_none() {
            self.req_sender.take().unwrap().send(req).unwrap();
            Box::new(self.res_receiver.take().unwrap().then(|r| match r {
                Ok(f) => f,
                Err(_) => unreachable!(),
            }))
        } else {
            Box::new(
                self.client
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .send_request(req)
                    .from_err(),
            )
        }
    }
}
