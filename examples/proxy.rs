extern crate nekit;
extern crate tokio;
extern crate trust_dns_resolver;

use nekit::{acceptor::*, connector::*, session::Session, utils::http::HttpProxyRewriterBuilder};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::prelude::*;
use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts},
    AsyncResolver,
};

fn main() {
    let addr = "127.0.0.1:9098".parse().unwrap();
    let listener = TcpListener::bind(&addr).expect("Bind to address failed");

    let (resolver, background) =
        AsyncResolver::new(ResolverConfig::default(), ResolverOpts::default());

    let server = listener
        .incoming()
        .map_err(|err| eprintln!("Accept failed: {:?})", err))
        .for_each(move |sock| {
            println!("Got a new connection!");

            let session = Arc::new(Mutex::new(Session::new()));

            let acceptor = HttpAcceptor::new(sock);

            let resolver = resolver.clone();

            let acceptor = acceptor
                .handshake(session.clone())
                .and_then(move |mid_result| {
                    let lock = session.lock().unwrap();
                    TcpConnector::new(resolver)
                        .connect(lock.endpoint.as_ref().unwrap())
                        .map(|p| (mid_result, p))
                        .from_err()
                })
                .and_then(|(mid_result, p)| {
                    mid_result.complete_with(p, HttpProxyRewriterBuilder {})
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
