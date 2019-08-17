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

use crate::{
    acceptor::Acceptor,
    core::{Endpoint, Error, Result},
};
use futures::{
    future::{BoxFuture, FutureExt, TryFutureExt},
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
};
use std::net::{IpAddr, SocketAddr};

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

pub struct Socks5Acceptor<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    io: T,
}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Socks5Acceptor<T> {
    pub fn new(io: T) -> Self {
        Socks5Acceptor { io }
    }
}

async fn handshake<T: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
    acceptor: Socks5Acceptor<T>,
) -> Result<Socks5MidHandshake<T>> {
    let mut buf = [0; 2];
    let mut io = acceptor.io;
    io.read_exact(&mut buf).err_into::<Error>().await?;

    if buf[0] != 5 {
        return Err(Socks5Error::UnsupportedVersion.into());
    }

    if buf[1] == 0 {
        return Err(Socks5Error::InvalidMethodCount.into());
    }

    let mut buf = vec![0, buf[1].into()];

    io.read_exact(&mut buf).err_into::<Error>().await?;

    if !buf.iter().any(|x| *x == 0) {
        return Err(Socks5Error::UnsupportedAuthMethod.into());
    }

    let buf: [u8; 2] = [5, 0];
    io.write_all(&buf).err_into::<Error>().await?;

    let mut buf = [0; 4];
    io.read_exact(&mut buf).err_into::<Error>().await?;

    if buf[0] != 5 {
        return Err(Socks5Error::UnsupportedVersion.into());
    }

    if buf[1] != 1 {
        return Err(Socks5Error::UnsupportedCommand.into());
    }

    let ip_or_domain = match buf[3] {
        1 => {
            let mut buf = [0; 4];
            io.read_exact(&mut buf).err_into::<Error>().await?;
            IpAddr::from(buf).into()
        }
        3 => {
            let mut buf = [0; 1];
            io.read_exact(&mut buf).err_into::<Error>().await?;

            let mut buf = vec![0; buf[0].into()];

            io.read_exact(&mut buf).await?;
            let domain = String::from_utf8(buf).map_err(Into::<Error>::into)?;
            IpOrDomain::Domain(domain)
        }
        4 => {
            let mut buf = [0; 16];
            io.read_exact(&mut buf).err_into::<Error>().await?;
            IpAddr::from(buf).into()
        }
        _ => return Err(Socks5Error::UnsupportedAddressType.into()),
    };

    let mut buf = [0; 2];
    io.read_exact(&mut buf).err_into::<Error>().await?;

    let port = u16::from_be_bytes(buf);
    return Ok(Socks5MidHandshake {
        io,
        target_endpoint: match ip_or_domain {
            IpOrDomain::Ip(ip) => Endpoint::new_from_addr(SocketAddr::new(ip, port)),
            IpOrDomain::Domain(d) => Endpoint::new_from_hostname(&d, port),
        },
    });
}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Acceptor<Socks5MidHandshake<T>>
    for Socks5Acceptor<T>
{
    fn handshake(self) -> BoxFuture<'static, Result<Socks5MidHandshake<T>>> {
        handshake(self).boxed()
    }
}

pub struct Socks5MidHandshake<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    io: T,
    target_endpoint: Endpoint,
}

async fn finalize<T: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
    mut acceptor: Socks5MidHandshake<T>,
) -> Result<T> {
    let buf = [5, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    acceptor.io.write_all(&buf).err_into::<Error>().await?;
    Ok(acceptor.io)
}

impl<I: AsyncRead + AsyncWrite + Send + Unpin + 'static> Socks5MidHandshake<I> {
    pub fn target_endpoint(&self) -> &Endpoint {
        &self.target_endpoint
    }

    pub fn finalize(self) -> BoxFuture<'static, Result<I>> {
        finalize(self).boxed()
    }
}
