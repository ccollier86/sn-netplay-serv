//! Shared Postgres connection helper for telemetry and analytics.
//!
//! This module owns TLS policy mapping so runtime writes and operator reports
//! connect to Postgres the same way.

use crate::observability::{PostgresDsn, PostgresTlsMode};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_postgres::{Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;
use tracing::warn;

/// Open Postgres client plus its background connection task.
pub struct PostgresConnection {
    /// Query client.
    pub client: Client,
    /// Background task that drives the socket.
    pub task: JoinHandle<()>,
}

/// Opens a Postgres connection using the DSN TLS policy.
pub async fn connect_postgres(
    dsn: &PostgresDsn,
) -> Result<PostgresConnection, PostgresConnectError> {
    match dsn.tls_mode() {
        PostgresTlsMode::Require => connect_tls_unverified(dsn).await,
        PostgresTlsMode::Verify => connect_tls_verified(dsn).await,
        PostgresTlsMode::Disable => connect_plain(dsn).await,
        PostgresTlsMode::Prefer => match connect_tls_unverified(dsn).await {
            Ok(connection) => Ok(connection),
            Err(error) => {
                warn!(%error, "postgres TLS connection failed; retrying plaintext because sslmode=prefer");
                connect_plain(dsn).await
            }
        },
    }
}

async fn connect_plain(dsn: &PostgresDsn) -> Result<PostgresConnection, PostgresConnectError> {
    let (client, connection) = tokio_postgres::connect(dsn.value(), NoTls).await?;
    let task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            warn!(%error, "postgres plaintext connection closed");
        }
    });

    Ok(PostgresConnection { client, task })
}

async fn connect_tls_verified(
    dsn: &PostgresDsn,
) -> Result<PostgresConnection, PostgresConnectError> {
    install_rustls_crypto_provider();
    let (connector, root_errors) = MakeRustlsConnect::with_native_certs()
        .map_err(|errors| PostgresConnectError::TlsRoots(format!("{errors:?}")))?;

    if !root_errors.is_empty() {
        warn!(
            errors = ?root_errors,
            "postgres TLS loaded native roots with nonfatal certificate-store errors"
        );
    }

    connect_with_tls_connector(dsn, connector, "verified").await
}

async fn connect_tls_unverified(
    dsn: &PostgresDsn,
) -> Result<PostgresConnection, PostgresConnectError> {
    install_rustls_crypto_provider();
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertificateVerifier));

    connect_with_tls_connector(dsn, MakeRustlsConnect::new(config), "TLS").await
}

async fn connect_with_tls_connector(
    dsn: &PostgresDsn,
    connector: MakeRustlsConnect,
    label: &'static str,
) -> Result<PostgresConnection, PostgresConnectError> {
    let (client, connection) = tokio_postgres::connect(dsn.value(), connector).await?;
    let task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            warn!(%error, "postgres {label} connection closed");
        }
    });

    Ok(PostgresConnection { client, task })
}

fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Debug)]
struct NoCertificateVerifier;

impl rustls::client::danger::ServerCertVerifier for NoCertificateVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Connection setup failure.
#[derive(Debug, thiserror::Error)]
pub enum PostgresConnectError {
    /// PostgreSQL connection failed.
    #[error("postgres connection failed")]
    Postgres(#[from] tokio_postgres::Error),
    /// Native TLS roots could not be loaded.
    #[error("postgres TLS root certificate loading failed: {0}")]
    TlsRoots(String),
}
