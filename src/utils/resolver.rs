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

use std::net::{IpAddr, SocketAddr};
use tokio::prelude::{future::ok, Future};
use trust_dns_resolver::AsyncResolver;
use utils::{endpoint::Endpoint, Error};

pub trait Resolver {
    fn resolve_hostname(
        &mut self,
        hostname: &str,
    ) -> Box<Future<Item = Vec<IpAddr>, Error = Error> + Send>;

    fn resolve_endpoint(
        &mut self,
        endpoint: &Endpoint,
    ) -> Box<Future<Item = Vec<SocketAddr>, Error = Error> + Send> {
        match endpoint {
            Endpoint::Ip(ref addr) => Box::new(ok(vec![addr.clone()])),
            Endpoint::HostName(ref hostname, port) => {
                let port = *port;
                Box::new(self.resolve_hostname(hostname).map(move |ipaddrs| {
                    ipaddrs
                        .iter()
                        .map(|ip| SocketAddr::new(*ip, port))
                        .collect()
                }))
            }
        }
    }
}

impl Resolver for AsyncResolver {
    fn resolve_hostname(
        &mut self,
        hostname: &str,
    ) -> Box<Future<Item = Vec<IpAddr>, Error = Error> + Send> {
        Box::new(
            self.lookup_ip(hostname)
                .map(|addrs| addrs.iter().collect())
                .from_err::<std::io::Error>()
                .from_err(),
        )
    }
}
