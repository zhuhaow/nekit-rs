use session::Session;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::io;
use tokio::prelude::*;
use utils::{forward, Endpoint, Error};

#[derive(Debug)]
enum Socks5Error {
    UnsupportedVersion,
    InvalidMethodCount,
    UnsupportedAuthMethod,
    UnsupportedCommand,
    UnsupportedAddressType,
}

impl std::fmt::Display for Socks5Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{:#?}", self)
    }
}

impl std::error::Error for Socks5Error {}

enum IpOrDomain {
    Ip(IpAddr),
    Domain(String),
}

impl From<IpAddr> for IpOrDomain {
    fn from(ip: IpAddr) -> IpOrDomain {
        IpOrDomain::Ip(ip)
    }
}

impl From<String> for IpOrDomain {
    fn from(domain: String) -> IpOrDomain {
        IpOrDomain::Domain(domain)
    }
}

pub struct Socks5Acceptor<Next: AsyncRead + AsyncWrite> {
    next: Next,
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static> Socks5Acceptor<Next> {
    pub fn new(next: Next) -> Self {
        Socks5Acceptor { next: next }
    }

    pub fn handshake(
        self,
        session: Arc<Mutex<Session>>,
    ) -> Box<Future<Item = MidHandshake<Next>, Error = Error> + Send> {
        Box::new(
            io::read_exact(self.next, [0; 2])
                .from_err()
                .and_then(
                    |(next, buf)| -> Box<Future<Item = (Next, Vec<u8>), Error = Error> + Send> {
                        if buf[0] != 5 {
                            return Box::new(
                                future::err(Socks5Error::UnsupportedVersion).from_err(),
                            );
                        }

                        if buf[1] == 0 {
                            return Box::new(
                                future::err(Socks5Error::InvalidMethodCount).from_err(),
                            );
                        }

                        Box::new(io::read_exact(next, vec![0; buf[1].into()]).from_err())
                    },
                ).and_then(
                    |(next, buf)| -> Box<Future<Item = (Next, [u8; 2]), Error = Error> + Send> {
                        if !buf.iter().any(|x| *x == 0) {
                            return Box::new(
                                future::err(Socks5Error::UnsupportedAuthMethod).from_err(),
                            );
                        }

                        let buf: [u8; 2] = [5, 0];
                        Box::new(io::write_all(next, buf).from_err())
                    },
                ).and_then(|(next, _)| io::read_exact(next, [0; 4]).from_err())
                .and_then(
                    |(next, buf)| -> Box<Future<Item = (Next, IpOrDomain), Error = Error> + Send> {
                        if buf[0] != 5 {
                            return Box::new(
                                future::err(Socks5Error::UnsupportedVersion).from_err(),
                            );
                        }

                        if buf[1] != 1 {
                            return Box::new(
                                future::err(Socks5Error::UnsupportedCommand).from_err(),
                            );
                        }

                        match buf[3] {
                            1 => Box::new(
                                io::read_exact(next, [0; 4])
                                    .map(|(next, buf)| (next, IpAddr::from(buf).into()))
                                    .from_err(),
                            ),
                            3 => Box::new(
                                io::read_exact(next, [0; 1])
                                    .and_then(|(next, buf)| {
                                        io::read_exact(next, vec![0; buf[0].into()])
                                    }).from_err()
                                    .and_then(|(next, buf)| {
                                        let domain = String::from_utf8(buf).map_err(Into::into);
                                        let domain = future::done(domain);
                                        domain.map(move |d| (next, IpOrDomain::Domain(d)))
                                    }),
                            ),
                            4 => Box::new(
                                io::read_exact(next, [0; 16])
                                    .map(|(next, buf)| (next, IpAddr::from(buf).into()))
                                    .from_err(),
                            ),
                            _ => Box::new(
                                future::err(Socks5Error::UnsupportedAddressType).from_err(),
                            ),
                        }
                    },
                ).and_then(|(next, ip_or_domain)| {
                    io::read_exact(next, [0; 2])
                        .map(move |r| (r, ip_or_domain))
                        .from_err()
                }).and_then(move |((next, buf), ip_or_domain)| {
                    // TODO: Use `from_be_bytes`
                    let mut port: u16 = buf[0].into();
                    port <<= 8;
                    port += u16::from(buf[1]);
                    session.lock().unwrap().endpoint = match ip_or_domain {
                        IpOrDomain::Ip(ip) => {
                            Endpoint::new_from_addr(SocketAddr::new(ip, port)).into()
                        }
                        IpOrDomain::Domain(d) => Endpoint::new_from_hostname(&d, port).into(),
                    };
                    future::ok(MidHandshake { next: next })
                }),
        )
    }
}

pub struct MidHandshake<Next: AsyncRead + AsyncWrite> {
    next: Next,
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static> MidHandshake<Next> {
    pub fn complete_with<Io: AsyncRead + AsyncWrite + Send + 'static>(
        self,
        io: Io,
    ) -> Box<Future<Item = (), Error = Error> + Send> {
        Box::new(
            io::write_all(self.next, [5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .map(|(p, _)| p)
                .and_then(move |p| forward(p, io))
                .map(|_| {})
                .from_err(),
        )
    }
}
