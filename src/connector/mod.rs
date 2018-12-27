mod tcp_connector;
pub use self::tcp_connector::TcpConnector;

use std::io::Error;
use tokio::prelude::*;
use utils::Endpoint;

pub trait Connector<P: AsyncRead + AsyncWrite + Send> {
    fn connect(self, endpoint: &Endpoint) -> Box<Future<Item = P, Error = Error> + Send>;
}
