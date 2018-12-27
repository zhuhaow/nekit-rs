use connector::Connector;
use tokio::{net::TcpStream, prelude::*};
use utils::{Endpoint, Error, Resolver};

pub struct TcpConnector<R: Resolver + Send> {
    resolver: R,
}

impl<R: Resolver + Send> TcpConnector<R> {
    pub fn new(resolver: R) -> Self {
        TcpConnector { resolver }
    }
}

impl<R: Resolver + Send> Connector<TcpStream> for TcpConnector<R> {
    fn connect(
        mut self,
        endpoint: &Endpoint,
    ) -> Box<Future<Item = TcpStream, Error = Error> + Send> {
        Box::new(
            self.resolver
                .resolve_endpoint(endpoint)
                // TODO: Try all IPs
                .and_then(|addrs| TcpStream::connect(addrs.first().unwrap()).from_err()),
        )
    }
}
