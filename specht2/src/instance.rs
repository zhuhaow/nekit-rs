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

use freighter::acceptor::http::HttpAcceptor;
use freighter::acceptor::http::HttpProxyTransformerBuilder;
use freighter::connector::Connector;
use freighter::connector::TcpConnector;
use freighter::resolver::AsyncResolver;
use specht2::connection::router::FnRouter;
use specht2::connection::router::Router;
use std::sync::{Arc, Mutex};
use tokio::net::tcp::TcpListener;
use tokio::prelude::*;
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};

fn main() {
    let addr = "127.0.0.1:9098".parse().unwrap();
    let listener = TcpListener::bind(&addr).expect("Bind to address failed");

    let (resolver, background) =
        AsyncResolver::new(ResolverConfig::default(), ResolverOpts::default());

    let router = Arc::new(Mutex::new(FnRouter::new(move |endpoint| {
        TcpConnector::new(resolver.clone()).connect(endpoint)
    })));

    let server = listener
        .incoming()
        .map_err(|err| eprintln!("Accept failed: {:?})", err))
        .for_each(move |sock| {
            println!("Got a new connection!");

            let acceptor = HttpAcceptor::new(sock);
            let mut router = Arc::clone(&router);

            let acceptor = acceptor
                .handshake()
                .and_then(move |mid_result| {
                    Arc::get_mut(&mut router)
                        .unwrap()
                        .get_mut()
                        .unwrap()
                        .route(mid_result.target_endpoint())
                        .map(|p| (mid_result, p))
                        .from_err()
                })
                .and_then(|(mid_result, p)| {
                    mid_result.complete_with(p, HttpProxyTransformerBuilder {})
                })
                .map_err(|err| eprintln!("Error: {:?}", err))
                .map(|_| {});
            tokio::spawn(acceptor)
        });

    tokio::run(
        server
            .select(background.map(|_| {}).map_err(|_| {}))
            .map(|_| {})
            .map_err(|_| {}),
    );
}
