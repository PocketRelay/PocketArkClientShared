//! HTTP server for safely forwarding HTTP requests that the client
//! makes along to the Pocket Relay server, since the game client
//! is only capable of communicating over SSLv3

use super::{spawn_server_task, HTTP_PORT};
use crate::api::{headers::X_TOKEN, proxy_http_request, AuthToken};
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
use url::Url;

/// Starts the HTTP proxy server
///
/// ## Arguments
/// * `http_client` - The HTTP client passed around for sending the requests
/// * `base_url`    - The server base URL to proxy requests to
/// * `context`     - The SSL context to use when accepting clients
/// * `token`       - The authentication token
pub async fn start_http_server(
    http_client: reqwest::Client,
    base_url: Arc<Url>,
    ssl_context: SslContext,
    token: AuthToken,
) -> anyhow::Result<()> {
    // Bind the local tcp socket for accepting connections
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, HTTP_PORT))
        .await
        .context("Failed to bind listener")?;

    // Accept connections
    loop {
        let (stream, _) = listener.accept().await?;

        let ssl = Ssl::new(&ssl_context).context("Failed to get ssl instance")?;
        let stream = SslStream::new(ssl, stream).context("Failed to create ssl stream")?;

        let http_client = http_client.clone();
        let base_url = base_url.clone();
        let token = token.clone();

        spawn_server_task(async move {
            if let Err(err) = serve_connection(stream, http_client, base_url, token).await {
                error!("Error while redirecting: {}", err);
            }
        });
    }
}

/// Handles serving an HTTP connection the provided `stream`, also
/// completes the accept stream process
pub async fn serve_connection(
    mut stream: SslStream<TcpStream>,
    http_client: reqwest::Client,
    base_url: Arc<Url>,
    token: AuthToken,
) -> anyhow::Result<()> {
    Pin::new(&mut stream).accept().await?;

    Http::new()
        .serve_connection(
            stream,
            service_fn(move |request| {
                handle(
                    request,
                    http_client.clone(),
                    base_url.clone(),
                    token.clone(),
                )
            }),
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
    http_client: reqwest::Client,
    base_url: Arc<Url>,
    token: AuthToken,
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
    let url = match base_url.join(path_and_query) {
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
        HeaderValue::from_str(&token).expect("Invalid token"),
    );

    // Proxy the request to the server
    let response = match proxy_http_request(&http_client, url, method, body, headers).await {
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
