// use std::convert::Infallible;
// use std::net::SocketAddr;
// use hyper::service::{make_service_fn, service_fn};
// use hyper::{Body, Client, Request, Response, Server};
// 
// type HttpClient = Client<hyper::client::HttpConnector>;
// 
// async fn get_response(client: HttpClient, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
//     let headers = req.headers().clone();
//     let mut request_builder = Request::builder()
//         .method(req.method())
//         .uri(req.uri())
//         .body(req.into_body())
//         .unwrap();
// 
//     *request_builder.headers_mut() = headers;
//     let response = client.request(request_builder).await?;
//     let body = hyper::body::to_bytes(response.into_body()).await?;
//     let body = String::from_utf8(body.to_vec()).unwrap();
// 
//     println!("Ok {body}");
//     let mut resp = Response::new(Body::from(body));
//     *resp.status_mut() = http::StatusCode::OK;
//     Ok(resp)
// }
// 
// async fn proxy(_client: HttpClient, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
//     let headers = req.headers().clone();
//     println!("headers: {:?}", headers);
//     println!("req: {:?}", req.uri().host());
//     if req.uri().host() == None{
//         println!("Should respond");
//         let resp = Response::new(Body::from("sorry! no route found"));
//         Ok(resp)
//     }else{
//         let resp = get_response(_client, req).await?;
//         Ok(resp)
//     }
// }
// #[tokio::main]
// async fn main() {
//     let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
// 
//     let client = Client::builder()
//         // .http1_title_case_headers(true)
//         // .http1_preserve_header_case(true)
//         .build_http();
// 
//     let make_service = make_service_fn(move |_| {
//         let client = client.clone();
//         async move { Ok::<_, Infallible>(service_fn(move |req| proxy(client.clone(), req))) }
//     });
// 
//     let server = Server::bind(&addr)
//         // .http1_preserve_header_case(true)
//         // .http1_title_case_headers(true)
//         .serve(make_service);
// 
//     println!("Listening on http://{}", addr);
// 
//     if let Err(e) = server.await {
//         eprintln!("server error: {}", e);
//     }
// }
use anyhow::{Result, anyhow};
use futures::io::{copy, AsyncReadExt, AsyncWriteExt};
use futures::stream::StreamExt;
use log::{info, error};
use smol::Async;

use std::net;
use std::net::ToSocketAddrs;

const PORT: u16 = 5000;

async fn handle_client(mut stream: Async<net::TcpStream>) -> Result<()> {
    info!("Request from {:?}", stream);

    // Open a TCP connection to the website
    let tunnel = {
        // Read the CONNECT request
        let mut buf = [0; 4096];
        let nbytes = stream.read(&mut buf).await?;

        let req = std::str::from_utf8(&buf[..nbytes])?;
        info!("Received request\n{}", req);

        let website = req
            .split_whitespace()
            .nth(1)
            .ok_or(anyhow!("invalid request"))?;

        info!("Connecting to {}", website);
        Async::<net::TcpStream>::connect(website.to_socket_addrs()?.next().unwrap()).await?
    };

    // Send an acknowledgement to the client
    stream
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .await?;

    // Keep copying data back and forth
    futures::future::try_join(
        copy(&mut &stream, &mut &tunnel),
        copy(&mut &tunnel, &mut &stream),
    )
    .await?;

    Ok(())
}

fn main() {
    env_logger::init();

    smol::block_on(async {
        // Create a server
        let server =
            Async::<net::TcpListener>::bind(format!("127.0.0.1:{}", PORT).parse::<net::SocketAddr>().unwrap()).unwrap_or_else(|e| {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    error!("Port {} is already being used by another program", PORT);
                    std::process::exit(1);
                } else {
                    panic!("{:?}", e);
                }
            });
        info!(
            "Listening on port {}",
            server.get_ref().local_addr().unwrap().port()
        );

        // Handle incoming connections
        server
            .incoming()
            .for_each_concurrent(None, |ev| async move {
                if let Ok(stream) = ev {
                    if let Err(error) = handle_client(stream).await {
                        error!("error while handling stream: {}", error);
                    }
                }
            })
            .await;
    })
}
