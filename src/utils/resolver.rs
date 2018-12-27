use std::net::{IpAddr, SocketAddr};
use tokio::prelude::{future::ok, Future};
use trust_dns_resolver::AsyncResolver;
use utils::{endpoint::Endpoint, Error};

pub trait Resolver {
    fn resolve_hostname(
        &mut self,
        hostname: &str,
    ) -> Box<Future<Item = Vec<IpAddr>, Error = Error> + Send>;

    fn resolve_endpoint(
        &mut self,
        endpoint: &Endpoint,
    ) -> Box<Future<Item = Vec<SocketAddr>, Error = Error> + Send> {
        match endpoint {
            Endpoint::Ip(ref addr) => Box::new(ok(vec![addr.clone()])),
            Endpoint::HostName(ref hostname, port) => {
                let port = *port;
                Box::new(self.resolve_hostname(hostname).map(move |ipaddrs| {
                    ipaddrs
                        .iter()
                        .map(|ip| SocketAddr::new(*ip, port))
                        .collect()
                }))
            }
        }
    }
}

impl Resolver for AsyncResolver {
    fn resolve_hostname(
        &mut self,
        hostname: &str,
    ) -> Box<Future<Item = Vec<IpAddr>, Error = Error> + Send> {
        Box::new(
            self.lookup_ip(hostname)
                .map(|addrs| addrs.iter().collect())
                .from_err::<std::io::Error>()
                .from_err(),
        )
    }
}
