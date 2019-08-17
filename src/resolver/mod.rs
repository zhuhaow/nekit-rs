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

use crate::core::{Endpoint, Result};
use futures::{
    compat::Future01CompatExt,
    future::{self, BoxFuture, FutureExt, TryFutureExt},
};
use std::net::{IpAddr, SocketAddr};

pub use trust_dns_resolver::AsyncResolver;

pub trait Resolver {
    fn resolve_hostname(self, hostname: &str) -> BoxFuture<Result<Vec<IpAddr>>>;

    fn resolve_endpoint(self, endpoint: &Endpoint) -> BoxFuture<Result<Vec<SocketAddr>>>
    where
        Self: Sized,
    {
        match *endpoint {
            Endpoint::Ip(ref addr) => future::ok(vec![*addr]).left_future(),
            Endpoint::HostName(ref hostname, port) => self
                .resolve_hostname(hostname)
                .map_ok(move |ipaddrs| {
                    ipaddrs
                        .iter()
                        .map(|ip| SocketAddr::new(*ip, port))
                        .collect()
                })
                .right_future(),
        }
        .boxed()
    }
}

impl Resolver for AsyncResolver {
    fn resolve_hostname(mut self, hostname: &str) -> BoxFuture<Result<Vec<IpAddr>>> {
        (&mut self)
            .lookup_ip(hostname)
            .compat()
            .map_ok(|addrs| addrs.iter().collect())
            .err_into::<std::io::Error>()
            .err_into()
            .boxed()
    }
}
