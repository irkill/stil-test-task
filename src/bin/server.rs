use std::time::Duration;
use std::convert::Infallible;
use hyper::{service, Body, Request, Response};
use rand::{self, Rng};

use stiltesttask::server::Server;

#[tokio::main]
async fn main() {
    let service = service::service_fn(handle);

    let server = Server::bind("127.0.0.1:31589")
        .max_connections(5)
        .http2_only(true)
        .service(service);

    server.run().await
}

async fn handle(_: Request<Body>) -> Result<Response<Body>, Infallible> {
    let d = Duration::from_millis(rand::thread_rng().gen_range(100..=500));
    tokio::time::sleep(d).await;
    Ok(Response::new("Some payload.".into()))
}
