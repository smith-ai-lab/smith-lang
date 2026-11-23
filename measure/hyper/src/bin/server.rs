//! The server `listen` is held against: `hyper` on `tokio`'s multi-threaded
//! runtime, which answers GET /users/:id with the JSON the Smith program does.
//! Usage: server PORT

use std::convert::Infallible;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

async fn answer(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let id = request.uri().path().rsplit('/').next().unwrap_or("0").to_string();
    let body = format!("{{\"id\":{id},\"name\":\"Ana\"}}");
    let response = Response::builder()
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap();
    Ok(response)
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args().nth(1).expect("a port").parse().expect("a number");
    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
    loop {
        let (stream, _) = listener.accept().await.expect("accept");
        tokio::spawn(async move {
            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service_fn(answer)).await;
        });
    }
}
