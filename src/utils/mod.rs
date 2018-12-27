mod endpoint;
pub mod http;
mod io;
mod resolver;

pub use self::endpoint::Endpoint;
pub use self::io::forward;
pub use self::resolver::Resolver;

use std::error::Error as StdError;

pub type Error = Box<dyn StdError + Sync + Send + 'static>;
