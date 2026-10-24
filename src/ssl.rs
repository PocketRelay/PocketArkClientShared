//! Stores helper functions for creating various SSL related
//! contexts

use anyhow::Context;
use openssl::{
    pkey::PKey,
    rsa::Rsa,
    ssl::{SslContext, SslMethod, SslVersion},
    x509::X509,
};
use rcgen::{CertificateParams, DnType, IsCa, Issuer, KeyPair, PKCS_RSA_SHA256};
use time::{Duration, OffsetDateTime};

/// Generate the self signed cert authority key
fn generate_ca() -> anyhow::Result<(CertificateParams, KeyPair)> {
    let mut ca_params = CertificateParams::default();
    let now = OffsetDateTime::now_utc();
    ca_params.not_before = now;
    ca_params.not_after = now + Duration::days(3650); // 10 years
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "GOS 2015 Certificate Authority");
    ca_params
        .distinguished_name
        .push(DnType::OrganizationName, "Electronic Arts, Inc. Ltd");
    ca_params.distinguished_name.push(DnType::CountryName, "US");
    ca_params
        .distinguished_name
        .push(DnType::StateOrProvinceName, "California");

    let ca_key_pair = KeyPair::generate_for(&PKCS_RSA_SHA256)?;
    Ok((ca_params, ca_key_pair))
}

/// Generate the self signed server certificate
fn generate_self_signed_certificate() -> anyhow::Result<(String, String)> {
    let (ca_params, ca_key_pair) = generate_ca()?;

    let mut server_params = CertificateParams::new(vec![
        "winter15.gosredirector.ea.com".to_string(),
        "spring18.gosredirector.ea.com".to_string(),
    ])?;
    let now = OffsetDateTime::now_utc();
    server_params.not_before = now;
    server_params.not_after = now + Duration::days(3650); // 10 years

    server_params
        .distinguished_name
        .push(DnType::CommonName, "winter15.gosredirector.ea.com");
    server_params
        .distinguished_name
        .push(DnType::OrganizationName, "Electronic Arts, Inc. Ltd");
    server_params
        .distinguished_name
        .push(DnType::CountryName, "US");
    server_params
        .distinguished_name
        .push(DnType::StateOrProvinceName, "California");

    let server_key_pair = KeyPair::generate_for(&PKCS_RSA_SHA256)?;

    let issuer = Issuer::new(ca_params, ca_key_pair);
    let server_cert = server_params.signed_by(&server_key_pair, &issuer)?;

    let server_cert_pem = server_cert.pem();
    let server_key_pem = server_key_pair.serialize_pem();

    Ok((server_cert_pem, server_key_pem))
}

/// Creates a new [SslContext] for use within a server context for
/// accepting connections
pub fn create_ssl_context() -> anyhow::Result<SslContext> {
    let (certificate_pem, private_key_pem) = generate_self_signed_certificate()?;

    let certificate =
        X509::from_pem(certificate_pem.as_bytes()).context("Failed to load certificate")?;
    let private_key = Rsa::private_key_from_pem(private_key_pem.as_bytes())
        .context("Failed to load private key")?;
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

#[cfg(test)]
mod test {
    use crate::ssl::generate_self_signed_certificate;

    #[test]
    fn test_generate_certificate() {
        generate_self_signed_certificate().unwrap();
    }
}
