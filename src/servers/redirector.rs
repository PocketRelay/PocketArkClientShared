//! Pocket Ark version of winter15.gosredirector.ea.com, informs the game clients
//! where the blaze server is located, in this case it always reports the
//! servers as localhost

use super::{spawn_server_task, BLAZE_PORT, REDIRECTOR_PORT};
use anyhow::Context;
use hyper::{
    header::{self, HeaderName, HeaderValue},
    server::conn::Http,
    service::service_fn,
    Body, HeaderMap, Request, Response, StatusCode,
};
use log::error;
use openssl::ssl::{Ssl, SslContext};
use std::{convert::Infallible, net::Ipv4Addr, pin::Pin};
use tokio::net::{TcpListener, TcpStream};
use tokio_openssl::SslStream;

/// Starts the redirector server
///
/// ## Arguments
/// * `context` - The SSL context to use when accepting clients
pub async fn start_redirector_server(ssl_context: SslContext) -> anyhow::Result<()> {
    // Bind the local tcp socket for accepting connections
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, REDIRECTOR_PORT))
        .await
        .context("Failed to bind listener")?;

    // Accept connections
    loop {
        let (stream, _) = listener.accept().await?;

        let ssl = Ssl::new(&ssl_context).context("Failed to get ssl instance")?;
        let stream = SslStream::new(ssl, stream).context("Failed to create ssl stream")?;

        spawn_server_task(async move {
            if let Err(err) = serve_connection(stream).await {
                error!("Error while redirecting: {}", err);
            }
        });
    }
}

/// Handles serving an HTTP connection the provided `stream`, also
/// completes the accept stream process
pub async fn serve_connection(mut stream: SslStream<TcpStream>) -> anyhow::Result<()> {
    Pin::new(&mut stream).accept().await?;

    Http::new()
        .serve_connection(stream, service_fn(handle_redirect))
        .await
        .context("Serve error")?;

    Ok(())
}

async fn handle_redirect(req: Request<hyper::body::Body>) -> Result<Response<Body>, Infallible> {
    // Handle unexpected requests
    if req.uri().path() != "/redirector/getServerInstance" {
        let mut response = Response::new(hyper::body::Body::empty());
        *response.status_mut() = StatusCode::NOT_FOUND;

        return Ok(response);
    }

    let ip = u32::from_be_bytes([127, 0, 0, 1]);
    let port = BLAZE_PORT;

    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
    <serverinstanceinfo>
        <address member="0">
            <valu>
                <hostname>localhost</hostname>
                <ip>{ip}</ip>
                <port>{port}</port>
            </valu>
        </address>
        <secure>0</secure>
        <trialservicename></trialservicename>
        <defaultdnsaddress>0</defaultdnsaddress>
    </serverinstanceinfo>"#
    );

    let headers: HeaderMap = [
        (
            HeaderName::from_static("x-blaze-component"),
            HeaderValue::from_static("redirector"),
        ),
        (
            HeaderName::from_static("x-blaze-command"),
            HeaderValue::from_static("getServerInstance"),
        ),
        (
            HeaderName::from_static("x-blaze-seqno"),
            HeaderValue::from_static("0"),
        ),
        (
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml"),
        ),
    ]
    .into_iter()
    .collect();

    let mut response = Response::new(hyper::body::Body::from(body));
    *response.headers_mut() = headers;

    Ok(response)
}
