//! HTTP server for safely forwarding HTTP requests that the client
//! makes along to the Pocket Relay server, since the game client
//! is only capable of communicating over SSLv3

use super::{spawn_server_task, HTTP_PORT};
use crate::{
    api::{headers::X_TOKEN, proxy_http_request},
    ctx::ClientContext,
};
use anyhow::Context;
use hyper::{
    body::HttpBody, header::HeaderValue, http::uri::PathAndQuery, server::conn::Http,
    service::service_fn, Body, Request, Response, StatusCode,
};
use log::error;
use openssl::ssl::{Ssl, SslContext};
use std::{convert::Infallible, net::Ipv4Addr, pin::Pin, sync::Arc};
use tokio::net::{TcpListener, TcpStream};
use tokio_openssl::SslStream;

/// Starts the HTTP proxy server
///
/// ## Arguments
/// * `http_client` - The HTTP client passed around for sending the requests
/// * `base_url`    - The server base URL to proxy requests to
/// * `context`     - The SSL context to use when accepting clients
/// * `token`       - The authentication token
pub async fn start_http_server(
    ctx: Arc<ClientContext>,
    ssl_context: SslContext,
) -> std::io::Result<()> {
    // Bind the local tcp socket for accepting connections
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, HTTP_PORT)).await?;

    // Accept connections
    loop {
        let (stream, _) = listener.accept().await?;

        let ssl = Ssl::new(&ssl_context).map_err(std::io::Error::other)?;
        let stream = SslStream::new(ssl, stream).map_err(std::io::Error::other)?;

        let ctx = ctx.clone();

        spawn_server_task(async move {
            if let Err(err) = serve_connection(stream, ctx).await {
                error!("Error while redirecting: {}", err);
            }
        });
    }
}

/// Handles serving an HTTP connection the provided `stream`, also
/// completes the accept stream process
pub async fn serve_connection(
    mut stream: SslStream<TcpStream>,
    ctx: Arc<ClientContext>,
) -> anyhow::Result<()> {
    Pin::new(&mut stream).accept().await?;

    Http::new()
        .serve_connection(
            stream,
            service_fn(move |request| handle(request, ctx.clone())),
        )
        .await
        .context("Serve error")?;

    Ok(())
}

/// Handles an HTTP request from the HTTP server proxying it along
/// to the Pocket Relay server
///
/// ## Arguments
/// * `request`     - The HTTP request
/// * `http_client` - The HTTP client to proxy the request with
/// * `base_url`    - The server base URL (Connection URL)
async fn handle(
    mut request: Request<Body>,
    ctx: Arc<ClientContext>,
) -> Result<Response<Body>, Infallible> {
    let path_and_query = request
        .uri()
        // Extract the path and query portion of the url
        .path_and_query()
        // Convert the path to a &str
        .map(PathAndQuery::as_str)
        // Fallback to empty path if none is provided
        .unwrap_or_default();

    // Strip the leading slash if one is present
    let path_and_query = path_and_query.strip_prefix('/').unwrap_or(path_and_query);

    // Create the new url from the path
    let url = match ctx.base_url.join(path_and_query) {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to create HTTP proxy URL: {}", err);

            let mut response = Response::default();
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            return Ok(response);
        }
    };

    let method = request.method().clone();

    let body = match request.body_mut().data().await.transpose() {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to read HTTP request body: {}", err);

            let mut response = Response::default();
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            return Ok(response);
        }
    };

    let mut headers = request.headers().clone();
    headers.insert(
        X_TOKEN,
        HeaderValue::from_str(&ctx.token).expect("Invalid token"),
    );

    // Proxy the request to the server
    let response = match proxy_http_request(&ctx.http_client, url, method, body, headers).await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to proxy HTTP request: {}", err);

            let mut response = Response::default();
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return Ok(response);
        }
    };

    Ok(response)
}
