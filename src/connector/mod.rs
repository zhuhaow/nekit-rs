mod tcp_connector;
pub use self::tcp_connector::TcpConnector;

use tokio::prelude::*;
use utils::{Endpoint, Error};

pub trait Connector<P: AsyncRead + AsyncWrite + Send> {
    fn connect(self, endpoint: &Endpoint) -> Box<Future<Item = P, Error = Error> + Send>;
}
