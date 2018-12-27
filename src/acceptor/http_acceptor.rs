use futures::{future::Either, sync::oneshot};
use hyper::{
    body::Body,
    client::conn as client_conn,
    server::conn::{Connection as ServerConnection, Http},
    service::Service,
    Method, Request, Response,
};
use session::Session;
use std::sync::{Arc, Mutex};
use tokio::{io, prelude::*};
use utils::{
    forward,
    http::{Client, ClientBuilder},
    Endpoint, Error,
};

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

    pub fn handshake(
        self,
        session: Arc<Mutex<Session>>,
    ) -> Box<Future<Item = MidHandshake<Next>, Error = Error> + Send> {
        let (req_sender, req_receiver) = oneshot::channel();
        let (res_sender, res_receiver) = oneshot::channel();

        let client: Arc<Mutex<Option<Box<dyn Client + Send>>>> = Arc::new(Mutex::new(None));
        let service = HttpService::new(req_sender, res_receiver, client.clone());
        let connection = Http::new().serve_connection(self.next, service);
        let fill_session =
            req_receiver
                .map_err(|_| panic!())
                .and_then(move |req| match req.method() {
                    &Method::CONNECT => {
                        if let (Some(host), Some(port)) = (
                            req.uri().host().map(|h| h.to_string()),
                            req.uri().port_part().map(|p| p.as_u16()),
                        ) {
                            session.lock().as_mut().unwrap().endpoint =
                                Endpoint::HostName(host, port).into();
                            future::ok(MidHandshake {
                                res_channel: res_sender.into(),
                                is_connect: true,
                                connection: None,
                                first_req: req,
                                client: client,
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
                            } else if req.uri().scheme_part().map(|s| s.as_str()) == Some("https") {
                                port = 443;
                            }

                            session.lock().as_mut().unwrap().endpoint =
                                Endpoint::HostName(host, port).into();
                            future::ok(MidHandshake {
                                res_channel: res_sender.into(),
                                is_connect: false,
                                connection: None,
                                first_req: req,
                                client: client,
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
                }).and_then(|r| match r {
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

pub struct MidHandshake<Next: AsyncRead + AsyncWrite + Send + 'static> {
    res_channel: Option<oneshot::Sender<Box<Future<Item = Response<Body>, Error = Error> + Send>>>,
    is_connect: bool,
    connection: Option<ServerConnection<Next, HttpService>>,
    first_req: Request<Body>,
    client: Arc<Mutex<Option<Box<dyn Client + Send>>>>,
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static> MidHandshake<Next> {
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
                ).and_then(move |(next, _)| forward(next, io))
                .map(|_| {})
                .from_err(),
            )
        } else {
            Box::new(client_conn::handshake(io).from_err().and_then(
                move |(handler, connection)| {
                    let mut client = client_builder.build(handler);
                    assert!(
                        self.res_channel
                            .unwrap()
                            .send(Box::new(client.send_request(self.first_req).from_err()))
                            .is_ok()
                    );
                    *self.client.lock().unwrap() = Some(client);

                    self.connection
                        .unwrap()
                        .select2(connection)
                        .map_err(|e| match e {
                            Either::A((err, _)) => Box::new(err) as Error,
                            Either::B((err, _)) => Box::new(err),
                        }).and_then(|r| match r {
                            Either::A(_) => Box::new(future::ok(()))
                                as Box<Future<Item = (), Error = Error> + Send>,
                            Either::B((_, conn)) => Box::new(conn.from_err()),
                        })
                },
            ))
        }
    }
}

struct HttpService {
    client: Arc<Mutex<Option<Box<dyn Client + Send>>>>,
    req_sender: Option<oneshot::Sender<Request<Body>>>,
    res_receiver:
        Option<oneshot::Receiver<Box<dyn Future<Item = Response<Body>, Error = Error> + Send>>>,
}

impl HttpService {
    fn new(
        req_sender: oneshot::Sender<Request<Body>>,
        res_receiver: oneshot::Receiver<Box<Future<Item = Response<Body>, Error = Error> + Send>>,
        client: Arc<Mutex<Option<Box<dyn Client + Send>>>>,
    ) -> Self {
        HttpService {
            client: client,
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
                Err(_) => panic!(),
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
