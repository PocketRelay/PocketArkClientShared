//! API logic for HTTP requests that are sent to the Pocket Relay server

use crate::MIN_SERVER_VERSION;
use bytes::Bytes;
use hyper::{
    header::{self, HeaderName, HeaderValue},
    Body, HeaderMap, Method, Response,
};
use log::error;
use reqwest::{Client, Identity, Upgraded};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{path::Path, str::FromStr, sync::Arc};
use thiserror::Error;
use url::Url;

use self::headers::X_TOKEN;

/// Endpoint used for requesting the server details
pub const DETAILS_ENDPOINT: &str = "api/server";
/// Endpoint for upgrading the server connection
pub const UPGRADE_ENDPOINT: &str = "api/server/upgrade";
/// Endpoint for creating an account
pub const CREATE_ACCOUNT_ENDPOINT: &str = "api/server/create";
/// Endpoint for logging into an account
pub const LOGIN_ENDPOINT: &str = "api/server/login";
/// Endpoint for creating a connection tunnel
pub const TUNNEL_ENDPOINT: &str = "api/server/tunnel";

/// Server identifier for validation
pub const SERVER_IDENT: &str = "POCKET_ARK_SERVER";

/// Client user agent created from the name and version
pub const USER_AGENT: &str = concat!("PocketArkClient/v", env!("CARGO_PKG_VERSION"));

/// Headers used by the client
pub mod headers {
    /// Header used for association tokens
    pub const ASSOCIATION: &str = "x-association";
    /// Header used for auth tokens
    pub const X_TOKEN: &str = "x-token";
}

/// Creates a new HTTP client to use, will use the client identity
/// if one is provided
///
/// ## Arguments
/// * `identity` - Optional identity for the client to use
pub fn create_http_client(identity: Option<Identity>) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder().user_agent(USER_AGENT);

    if let Some(identity) = identity {
        builder = builder.identity(identity);
    }

    builder.build()
}

/// Errors that can occur when loading the client identity
#[derive(Debug, Error)]
pub enum ClientIdentityError {
    /// Failed to read the identity file
    #[error("Failed to read identity: {0}")]
    Read(#[from] std::io::Error),
    /// Failed to create the identity
    #[error("Failed to create identity: {0}")]
    Create(#[from] reqwest::Error),
}

/// Attempts to read a client identity from the provided file path,
/// the file must be a .p12 / .pfx (PKCS12) format containing a
/// certificate and private key with a blank password
///
/// ## Arguments
/// * `path` - The path to read the identity from
pub fn read_client_identity(path: &Path) -> Result<Identity, ClientIdentityError> {
    // Read the identity file bytes
    let bytes = std::fs::read(path).map_err(ClientIdentityError::Read)?;

    // Parse the identity from the file bytes
    Identity::from_pkcs12_der(&bytes, "").map_err(ClientIdentityError::Create)
}

/// Details provided by the server. These are the only fields
/// that we need the rest are ignored by this client.
#[derive(Deserialize)]
struct ServerDetails {
    /// The Pocket Relay version of the server
    version: Version,
    /// Server identifier checked to ensure its a proper server
    #[serde(default)]
    ident: Option<String>,
    /// Association token if the server supports providing one
    association: Option<String>,
}

/// Data from completing a lookup contains the resolved address
/// from the connection to the server as well as the server
/// version obtained from the server
#[derive(Debug, Clone)]
pub struct LookupData {
    /// Server url
    pub url: Arc<Url>,
    /// The server version
    pub version: Version,
    /// Association token if the server supports providing one
    pub association: Arc<Option<String>>,
}

/// Errors that can occur while looking up a server
#[derive(Debug, Error)]
pub enum LookupError {
    /// The server url was invalid
    #[error("Invalid Connection URL: {0}")]
    InvalidHostTarget(#[from] url::ParseError),
    /// The server connection failed
    #[error("Failed to connect to server: {0}")]
    ConnectionFailed(reqwest::Error),
    /// The server gave an invalid response likely not a PR server
    #[error("Server replied with error response: {0}")]
    ErrorResponse(reqwest::Error),
    /// The server gave an invalid response likely not a PR server
    #[error("Invalid server response: {0}")]
    InvalidResponse(reqwest::Error),
    /// Server wasn't a valid pocket relay server
    #[error("Server identifier was incorrect (Not a PocketArk server?)")]
    NotPocketArk,
    /// Server version is too old
    #[error("Server version is too outdated ({0}) this client requires servers of version {1} or greater")]
    ServerOutdated(Version, Version),
}

/// Attempts to lookup a server at the provided url to see if
/// its a Pocket Relay server
///
/// ## Arguments
/// * `http_client` - The HTTP client to connect with
/// * `base_url`    - The server base URL (Connection URL)
pub async fn lookup_server(
    http_client: reqwest::Client,
    host: String,
) -> Result<LookupData, LookupError> {
    let mut url = String::new();

    // Whether a scheme was inferred
    let mut inferred_scheme = false;

    // Fill in missing scheme portion
    if !host.starts_with("http://") && !host.starts_with("https://") {
        url.push_str("http://");

        inferred_scheme = true;
    }

    url.push_str(&host);

    // Ensure theres a trailing slash (URL path will be interpeted incorrectly without)
    if !url.ends_with('/') {
        url.push('/');
    }

    let mut url = Url::from_str(&url)?;

    // Update scheme to be https if the 443 port was specified and the scheme was inferred as http://
    if url.port().is_some_and(|port| port == 443) && inferred_scheme {
        let _ = url.set_scheme("https");
    }

    let info_url = url
        .join(DETAILS_ENDPOINT)
        .expect("Failed to create server details URL");

    // Send the HTTP request and get its response
    let response = http_client
        .get(info_url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(LookupError::ConnectionFailed)?;

    // Debug printing of response details for debug builds
    #[cfg(debug_assertions)]
    {
        use log::debug;

        debug!("Response Status: {}", response.status());
        debug!("HTTP Version: {:?}", response.version());
        debug!("Content Length: {:?}", response.content_length());
        debug!("HTTP Headers: {:?}", response.headers());
    }

    // Ensure the response wasn't a non 200 response
    let response = response
        .error_for_status()
        .map_err(LookupError::ErrorResponse)?;

    // Parse the JSON serialized server details
    let details = response
        .json::<ServerDetails>()
        .await
        .map_err(LookupError::InvalidResponse)?;

    // Handle invalid server ident
    if details.ident.is_none() || details.ident.is_some_and(|value| value != SERVER_IDENT) {
        return Err(LookupError::NotPocketArk);
    }

    // Ensure the server is a supported version
    if details.version < MIN_SERVER_VERSION {
        return Err(LookupError::ServerOutdated(
            details.version,
            MIN_SERVER_VERSION,
        ));
    }

    // Debug logging association aquire
    #[cfg(debug_assertions)]
    {
        use log::debug;
        if let Some(association) = &details.association {
            debug!("Aquired association token: {}", association);
        }
    }

    Ok(LookupData {
        url: Arc::new(url),
        version: details.version,
        association: Arc::new(details.association),
    })
}

/// Errors that could occur when creating a server stream
#[derive(Debug, Error)]
pub enum ServerStreamError {
    /// Initial HTTP request failure
    #[error("Request failed: {0}")]
    RequestFailed(reqwest::Error),
    /// Server responded with an error message
    #[error("Server error response: {0}")]
    ServerError(reqwest::Error),
    /// Upgrading the connection failed
    #[error("Upgrade failed: {0}")]
    UpgradeFailure(reqwest::Error),
}

/// Creates a BlazeSDK upgraded stream using HTTP upgrades
/// with the Pocket Relay server
///
/// ## Arguments
/// * `http_client` - The HTTP client to connect with
/// * `base_url`    - The server base URL (Connection URL)
/// * `association` - Optional client association token
/// * `token`       - Authentication token
pub async fn create_server_stream(
    http_client: reqwest::Client,
    base_url: &Url,
    association: Option<&String>,
    token: AuthToken,
) -> Result<Upgraded, ServerStreamError> {
    // Create the upgrade endpoint URL
    let endpoint_url: Url = base_url
        .join(UPGRADE_ENDPOINT)
        .expect("Failed to create upgrade endpoint");

    // Headers to provide when upgrading
    let mut headers: HeaderMap<HeaderValue> = [
        (header::CONNECTION, HeaderValue::from_static("Upgrade")),
        (header::UPGRADE, HeaderValue::from_static("blaze")),
        (
            HeaderName::from_static(X_TOKEN),
            HeaderValue::from_str(&token).expect("Invalid token"),
        ),
    ]
    .into_iter()
    .collect();

    // Include association token
    if let Some(association) = association {
        headers.insert(
            HeaderName::from_static(headers::ASSOCIATION),
            HeaderValue::from_str(association).expect("Invalid association token"),
        );
    }

    // Send the HTTP request and get its response
    let response = http_client
        .get(endpoint_url)
        .headers(headers)
        .send()
        .await
        .map_err(ServerStreamError::RequestFailed)?;

    // Handle server error responses
    let response = response
        .error_for_status()
        .map_err(ServerStreamError::ServerError)?;

    // Upgrade the connection
    response
        .upgrade()
        .await
        .map_err(ServerStreamError::UpgradeFailure)
}

/// Request structure for creating a new user
#[derive(Debug, Serialize)]
pub struct CreateUserRequest {
    /// The email for the user to create
    pub email: String,
    /// The username for the user to create
    pub username: String,
    /// The password for the user to create
    pub password: String,
}

/// Errors that could occur when creating a server stream
#[derive(Debug, Error)]
pub enum ServerAuthError {
    /// Initial HTTP request failure
    #[error("Request failed: {0}")]
    RequestFailed(reqwest::Error),
    /// Server responded with an error message
    #[error("Server error response: {0} {0}")]
    ServerError(reqwest::Error, String),
    /// Server response was malformed
    #[error("Malformed server response: {0}")]
    Malformed(reqwest::Error),
}

/// Authentication token
pub type AuthToken = Arc<str>;

/// Response structure for a token auth response
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    /// The authorization token
    pub token: String,
}

/// Attempts to create a new user account, returns the
/// authentication token on success
///
/// ## Arguments
/// * `http_client` - The HTTP client to connect with
/// * `base_url`    - The server base URL (Connection URL)
/// * `request`     - The account creation request
pub async fn create_user(
    http_client: reqwest::Client,
    base_url: Url,
    request: CreateUserRequest,
) -> Result<AuthToken, ServerAuthError> {
    // Create the upgrade endpoint URL
    let endpoint_url: Url = base_url
        .join(CREATE_ACCOUNT_ENDPOINT)
        .expect("Failed to create new account endpoint");

    // Send the HTTP request and get its response
    let response = http_client
        .post(endpoint_url)
        .json(&request)
        .send()
        .await
        .map_err(ServerAuthError::RequestFailed)?;

    // Handle server error responses
    if let Err(err) = response.error_for_status_ref() {
        let text = response.text().await.ok();
        return Err(ServerAuthError::ServerError(err, text.unwrap_or_default()));
    };

    let response: TokenResponse = response.json().await.map_err(ServerAuthError::Malformed)?;
    Ok(Arc::<str>::from(response.token.as_str()))
}

/// Request structure for creating a new user
#[derive(Debug, Serialize)]
pub struct LoginUserRequest {
    /// The email for the user to login
    pub email: String,
    /// The password for the user to login
    pub password: String,
}

/// Attempts to create a new user account, returns the
/// authentication token on success
///
/// ## Arguments
/// * `http_client` - The HTTP client to connect with
/// * `base_url`    - The server base URL (Connection URL)
/// * `request`     - The account login request
pub async fn login_user(
    http_client: reqwest::Client,
    base_url: Url,
    request: LoginUserRequest,
) -> Result<AuthToken, ServerAuthError> {
    // Create the upgrade endpoint URL
    let endpoint_url: Url = base_url
        .join(LOGIN_ENDPOINT)
        .expect("Failed to create login account endpoint");

    // Send the HTTP request and get its response
    let response = http_client
        .post(endpoint_url)
        .json(&request)
        .send()
        .await
        .map_err(ServerAuthError::RequestFailed)?;

    // Handle server error responses
    if let Err(err) = response.error_for_status_ref() {
        let text = response.text().await.ok();
        return Err(ServerAuthError::ServerError(err, text.unwrap_or_default()));
    };

    let response: TokenResponse = response.json().await.map_err(ServerAuthError::Malformed)?;
    Ok(Arc::<str>::from(response.token.as_str()))
}

/// Errors that could occur when proxying a request
#[derive(Debug, Error)]
pub enum ProxyError {
    /// Initial HTTP request failure
    #[error("Request failed: {0}")]
    RequestFailed(reqwest::Error),
    /// Failed to read the response body bytes
    #[error("Request failed: {0}")]
    BodyFailed(reqwest::Error),
}

/// Proxies an HTTP request to the Pocket Relay server returning a
/// hyper response that can be served
///
/// ## Arguments
/// * `http_client` - The HTTP client to connect with
/// * `url`         - The server URL to request
pub async fn proxy_http_request(
    http_client: &reqwest::Client,
    url: Url,
    method: Method,
    body: Option<Bytes>,
    mut headers: HeaderMap,
) -> Result<Response<Body>, ProxyError> {
    // Remove conflicting headers
    headers.remove(header::TRANSFER_ENCODING);
    headers.remove(header::CONTENT_LENGTH);

    // Send the HTTP request and get its response
    let mut request = http_client
        .request(method, url)
        // Include the request headers
        .headers(headers);

    if let Some(body) = body {
        request = request.body(body);
    }

    let response = request.send().await.map_err(ProxyError::RequestFailed)?;

    // Extract response status and headers before its consumed to load the body
    let status = response.status();
    let headers = response.headers().clone();

    // Read the response body bytes
    let body: bytes::Bytes = response.bytes().await.map_err(ProxyError::BodyFailed)?;

    // Create new response from the proxy response
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;

    Ok(response)
}

/// Creates a networking tunnel for game packets
///
/// ## Arguments
/// * `http_client` - The HTTP client to connect with
/// * `base_url`    - The server base URL (Connection URL)
/// * `association` - Optional association token
pub async fn create_server_tunnel(
    http_client: reqwest::Client,
    base_url: &Url,
    association: Option<&String>,
) -> Result<Upgraded, ServerStreamError> {
    // Create the upgrade endpoint URL
    let endpoint_url: Url = base_url
        .join(TUNNEL_ENDPOINT)
        .expect("Failed to create tunnel endpoint");

    // Headers to provide when upgrading
    let mut headers: HeaderMap<HeaderValue> = [
        (header::CONNECTION, HeaderValue::from_static("Upgrade")),
        (header::UPGRADE, HeaderValue::from_static("tunnel")),
    ]
    .into_iter()
    .collect();

    // Include association token
    if let Some(association) = association {
        headers.insert(
            HeaderName::from_static(headers::ASSOCIATION),
            HeaderValue::from_str(association).expect("Invalid association token"),
        );
    }

    // Send the HTTP request and get its response
    let response = http_client
        .get(endpoint_url)
        .headers(headers)
        .send()
        .await
        .map_err(ServerStreamError::RequestFailed)?;

    // Handle server error responses
    let response = response
        .error_for_status()
        .map_err(ServerStreamError::ServerError)?;

    // Upgrade the connection
    response
        .upgrade()
        .await
        .map_err(ServerStreamError::UpgradeFailure)
}
