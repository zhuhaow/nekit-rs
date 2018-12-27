use connector::Connector;
use tokio::{net::TcpStream, prelude::*};
use utils::{Endpoint, Error, Resolver};

pub struct TcpConnector {
    resolver: Box<Resolver>,
}

impl TcpConnector {
    pub fn new(resolver: Box<Resolver>) -> Self {
        TcpConnector { resolver }
    }
}

impl Connector<TcpStream> for TcpConnector {
    fn connect(
        mut self,
        endpoint: &Endpoint,
    ) -> Box<Future<Item = TcpStream, Error = Error> + Send> {
        Box::new(
            self.resolver
                .resolve_endpoint(endpoint)
                .and_then(|addrs| TcpStream::connect(addrs.first().unwrap()).from_err()),
        )
    }
}
