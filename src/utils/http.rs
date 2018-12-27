use hyper::{body::Body, client::conn::SendRequest, Request, Response};
use tokio::prelude::*;
use utils::Error;

pub trait ClientBuilder {
    fn build(self, handler: SendRequest<Body>) -> Box<dyn Client + Send>;
}

pub trait Client {
    fn send_request(
        &mut self,
        request: Request<Body>,
    ) -> Box<Future<Item = Response<Body>, Error = Error> + Send>;
}

impl Client for SendRequest<Body> {
    fn send_request(
        &mut self,
        request: Request<Body>,
    ) -> Box<Future<Item = Response<Body>, Error = Error> + Send> {
        Box::new(self.send_request(request).from_err())
    }
}

pub struct NoOpClientBuilder {}

impl ClientBuilder for NoOpClientBuilder {
    fn build(self, handler: SendRequest<Body>) -> Box<dyn Client + Send> {
        Box::new(handler)
    }
}

pub struct HttpProxyRewriter {
    inner: SendRequest<Body>,
}

impl Client for HttpProxyRewriter {
    fn send_request(
        &mut self,
        mut request: Request<Body>,
    ) -> Box<Future<Item = Response<Body>, Error = Error> + Send> {
        let rel_uri = {
            request
                .uri()
                .path_and_query()
                .map(|p| p.as_str())
                .unwrap_or(&"/")
                .parse()
                .unwrap()
        };
        *request.uri_mut() = rel_uri;
        Box::new(self.inner.send_request(request).from_err())
    }
}

pub struct HttpProxyRewriterBuilder {}

impl ClientBuilder for HttpProxyRewriterBuilder {
    fn build(self, handler: SendRequest<Body>) -> Box<dyn Client + Send> {
        Box::new(HttpProxyRewriter { inner: handler })
    }
}
