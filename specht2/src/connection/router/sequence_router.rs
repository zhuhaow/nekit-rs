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

use super::{RouteError, Router};
use freighter::core::{Endpoint, Error};
use future_utils::StreamExt;
use futures::stream::iter_ok;
use std::sync::Arc;
use tokio::prelude::*;

pub struct SequenceRouter<P, Fut>
where
    P: AsyncRead + AsyncWrite + Send,
    Fut: Future<Item = P, Error = Error>,
{
    rules: Arc<Vec<Box<Router<Item = P, Fut = Fut>>>>,
}

impl<P, Fut> SequenceRouter<P, Fut>
where
    P: AsyncRead + AsyncWrite + Send,
    Fut: Future<Item = P, Error = Error>,
{
    pub fn new() -> Self {
        SequenceRouter {
            rules: Arc::new(Vec::new()),
        }
    }

    pub fn push(&mut self, router: Box<Router<Item = P, Fut = Fut>>) {
        Arc::get_mut(&mut self.rules).unwrap().push(router)
    }
}

impl<P, Fut> Router for SequenceRouter<P, Fut>
where
    P: AsyncRead + AsyncWrite + Send + 'static,
    Fut: Future<Item = P, Error = Error> + 'static,
{
    type Item = P;
    type Fut = Box<Future<Item = P, Error = Error> + 'static>;

    fn route(&self, endpoint: &Endpoint) -> Self::Fut {
        let rules = Arc::clone(&self.rules);
        let endpoint = endpoint.clone();
        Box::new(
            iter_ok::<_, Error>(0..(rules.len()))
                .map(move |i| rules[i].route(&endpoint).into_stream())
                .flatten()
                .first_ok()
                .map_err(|mut errs| {
                    errs.pop()
                        .unwrap_or(Box::new(RouteError::NoRouteToDestinaion))
                }),
        )
    }
}
