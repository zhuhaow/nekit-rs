extern crate nekit;
extern crate tokio;
extern crate trust_dns_resolver;

use nekit::{acceptor::*, connector::*};
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

            let acceptor = http::HttpAcceptor::new(sock);

            let resolver = resolver.clone();

            let acceptor = acceptor
                .handshake()
                .and_then(move |mid_result| {
                    TcpConnector::new(resolver)
                        .connect(mid_result.target_endpoint())
                        .map(|p| (mid_result, p))
                        .from_err()
                })
                .and_then(|(mid_result, p)| {
                    mid_result.complete_with(p, http::HttpProxyTransformerBuilder {})
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
