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

use super::Connector;
use crate::{
    core::{Endpoint, Result},
    resolver::Resolver,
};
use futures::future::{BoxFuture, FutureExt, TryFutureExt};
use tokio::net::TcpStream;

pub struct TcpConnector<R: Resolver + Send + 'static> {
    resolver: R,
}

impl<R: Resolver + Send + 'static> TcpConnector<R> {
    pub fn new(resolver: R) -> Self {
        TcpConnector { resolver }
    }
}

impl<R: Resolver + Send + 'static> Connector<TcpStream> for TcpConnector<R> {
    fn connect(self, endpoint: &Endpoint) -> BoxFuture<Result<TcpStream>> {
        self.resolver
            .resolve_endpoint(endpoint)
            // TODO: Try all IPs
            .map_ok(|addrs| addrs.first().unwrap().clone())
            .and_then(|addr| TcpStream::connect(&addr).err_into())
            .boxed()
    }
}
