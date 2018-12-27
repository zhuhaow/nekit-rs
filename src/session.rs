use utils::Endpoint;

pub struct Session {
    pub endpoint: Option<Endpoint>,
}

impl Session {
    pub fn new() -> Session {
        Session { endpoint: None }
    }
}
