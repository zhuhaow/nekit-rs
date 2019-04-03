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

mod no_op_client;
mod proxy_transform_client;

use hyper::{body::Body, client::conn::SendRequest, Request, Response};
use nekit_core::Error;
pub use no_op_client::NoOpClientBuilder;
pub use proxy_transform_client::HttpProxyTransformerBuilder;
use tokio::prelude::*;

pub trait ClientBuilder {
    fn build(self, handler: SendRequest<Body>) -> Box<dyn Client + Send>;
}

pub trait Client {
    fn send_request(
        &mut self,
        request: Request<Body>,
    ) -> Box<Future<Item = Response<Body>, Error = Error> + Send>;
}

impl Client for SendRequest<Body> {
    fn send_request(
        &mut self,
        request: Request<Body>,
    ) -> Box<Future<Item = Response<Body>, Error = Error> + Send> {
        Box::new(self.send_request(request).from_err())
    }
}
