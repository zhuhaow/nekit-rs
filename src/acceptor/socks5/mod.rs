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

use crate::acceptor::Acceptor;
use crate::acceptor::MidHandshake;
use crate::{
    core::{Endpoint, Error},
    io::forward,
};
use std::net::{IpAddr, SocketAddr};
use tokio::{io, prelude::*};

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
        Socks5Acceptor { next }
    }
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static>
    Acceptor<
        Socks5MidHandshake<Next>,
        Box<Future<Item = Socks5MidHandshake<Next>, Error = Error> + Send>,
    > for Socks5Acceptor<Next>
{
    fn handshake(self) -> Box<Future<Item = Socks5MidHandshake<Next>, Error = Error> + Send> {
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
                )
                .and_then(
                    |(next, buf)| -> Box<Future<Item = (Next, [u8; 2]), Error = Error> + Send> {
                        if !buf.iter().any(|x| *x == 0) {
                            return Box::new(
                                future::err(Socks5Error::UnsupportedAuthMethod).from_err(),
                            );
                        }

                        let buf: [u8; 2] = [5, 0];
                        Box::new(io::write_all(next, buf).from_err())
                    },
                )
                .and_then(|(next, _)| io::read_exact(next, [0; 4]).from_err())
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
                                    })
                                    .from_err()
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
                )
                .and_then(|(next, ip_or_domain)| {
                    io::read_exact(next, [0; 2])
                        .map(move |r| (r, ip_or_domain))
                        .from_err()
                })
                .and_then(move |((next, buf), ip_or_domain)| {
                    let port = u16::from_be_bytes(buf);
                    future::ok(Socks5MidHandshake {
                        next,
                        target_endpoint: match ip_or_domain {
                            IpOrDomain::Ip(ip) => {
                                Endpoint::new_from_addr(SocketAddr::new(ip, port))
                            }
                            IpOrDomain::Domain(d) => Endpoint::new_from_hostname(&d, port),
                        },
                    })
                }),
        )
    }
}

pub struct Socks5MidHandshake<Next: AsyncRead + AsyncWrite> {
    next: Next,
    target_endpoint: Endpoint,
}

impl<Next: AsyncRead + AsyncWrite + Send + 'static> MidHandshake for Socks5MidHandshake<Next> {
    fn target_endpoint(&self) -> &Endpoint {
        &self.target_endpoint
    }

    fn complete_with<Io: AsyncRead + AsyncWrite + Send + 'static>(
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
