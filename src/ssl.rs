//! Stores helper functions for creating various SSL related
//! contexts

use anyhow::Context;
use openssl::{
    pkey::PKey,
    rsa::Rsa,
    ssl::{SslContext, SslMethod, SslVersion},
    x509::X509,
};

/// Creates a new [SslContext] for use within a server context for
/// accepting connections
pub fn create_ssl_context() -> anyhow::Result<SslContext> {
    const CERTIFICATE_BYTES: &[u8] = include_bytes!("pocket_ark.crt");
    const PRIVATE_KEY_BYTES: &[u8] = include_bytes!("pocket_ark.key");

    let certificate = X509::from_der(CERTIFICATE_BYTES).context("Failed to load certificate")?;
    let private_key =
        Rsa::private_key_from_pem(PRIVATE_KEY_BYTES).context("Failed to load private key")?;
    let private_key = PKey::from_rsa(private_key).context("Failed to create private key")?;

    let mut builder =
        SslContext::builder(SslMethod::tls_server()).context("Failed to create ssl context")?;

    // Set the certificate and private key
    builder.set_certificate(&certificate)?;
    builder.set_private_key(&private_key)?;

    // Ensure the server uses TLSv1.2
    builder.set_min_proto_version(Some(SslVersion::TLS1_2))?;
    builder.set_max_proto_version(Some(SslVersion::TLS1_2))?;

    Ok(builder.build())
}
