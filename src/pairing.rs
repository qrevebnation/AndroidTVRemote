use bytes::BytesMut;
use rsa::pkcs8::DecodePublicKey;
use rsa::traits::PublicKeyParts;
use rsa::RsaPublicKey;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::ClientConfig;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use crate::cert::NoVerifyServerCert;
use crate::codec::{decode_frame, encode_frame};
use crate::proto::polo::*;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(10);
const USER_PIN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("Invalid PIN format: must be 6 hex characters")]
    InvalidPinFormat,
    #[error("PIN hash mismatch: entered PIN does not match expected server challenge")]
    PinMismatch,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),
    #[error("Decode error: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Channel error")]
    ChannelClosed,
    #[error("Network operation timed out")]
    Timeout,
}

/// Computes the SHA-256 pairing secret from client/server RSA modulus & exponent and 6-hex-char PIN.
pub fn calculate_pairing_secret(
    client_mod: &[u8],
    client_exp: &[u8],
    server_mod: &[u8],
    server_exp: &[u8],
    pin: &str,
) -> Result<[u8; 32], PairingError> {
    if pin.len() != 6 {
        return Err(PairingError::InvalidPinFormat);
    }

    let pin_bytes = hex::decode(pin).map_err(|_| PairingError::InvalidPinFormat)?;
    let pin_prefix = pin_bytes[0];
    let pin_tail = &pin_bytes[1..];

    let mut hasher = Sha256::new();
    hasher.update(client_mod);
    hasher.update(client_exp);
    hasher.update(server_mod);
    hasher.update(server_exp);
    hasher.update(pin_tail);

    let result: [u8; 32] = hasher.finalize().into();
    if result[0] != pin_prefix {
        return Err(PairingError::PinMismatch);
    }

    Ok(result)
}

/// Extracts RSA modulus and exponent from a DER-encoded X.509 certificate.
pub fn extract_rsa_pubkey(cert_der: &[u8]) -> Result<(Vec<u8>, Vec<u8>), PairingError> {
    let cert = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|e| PairingError::Protocol(format!("X509 parse error: {e}")))?;
    let spki_der = cert.1.tbs_certificate.subject_pki.raw;

    let pubkey = RsaPublicKey::from_public_key_der(spki_der)
        .map_err(|e| PairingError::Protocol(format!("RSA pubkey parse error: {e}")))?;

    let modulus = pubkey.n().to_bytes_be();
    let exponent = pubkey.e().to_bytes_be();
    Ok((modulus, exponent))
}

pub async fn start_pairing(
    tv_ip: &str,
    client_cert: &CertificateDer<'static>,
    client_key: &PrivateKeyDer<'static>,
    pin_rx: tokio::sync::oneshot::Receiver<String>,
) -> Result<(), PairingError> {
    // Official Android TV Remote Pairing port is 6467
    start_pairing_with_port(tv_ip, 6467, client_cert, client_key, pin_rx).await
}

pub async fn start_pairing_with_port(
    tv_ip: &str,
    port: u16,
    client_cert: &CertificateDer<'static>,
    client_key: &PrivateKeyDer<'static>,
    pin_rx: tokio::sync::oneshot::Receiver<String>,
) -> Result<(), PairingError> {
    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifyServerCert))
        .with_client_auth_cert(vec![client_cert.clone()], client_key.clone_key())?;

    config.alpn_protocols = vec![b"atvremote".to_vec()];

    let connector = TlsConnector::from(Arc::new(config));
    let addr = format!("{}:{}", tv_ip, port);
    let tcp = timeout(NETWORK_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| PairingError::Timeout)??;
    let domain = ServerName::try_from("atvremote".to_string())
        .map_err(|_| PairingError::Protocol("Invalid server name".to_string()))?;

    let mut tls = timeout(NETWORK_TIMEOUT, connector.connect(domain, tcp))
        .await
        .map_err(|_| PairingError::Timeout)??;

    let server_cert = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| certs.first())
        .ok_or_else(|| PairingError::Protocol("No server certificate received".to_string()))?
        .clone();

    let (client_mod, client_exp) = extract_rsa_pubkey(client_cert.as_ref())?;
    let (server_mod, server_exp) = extract_rsa_pubkey(server_cert.as_ref())?;

    let mut write_buf = BytesMut::new();
    let mut read_buf = BytesMut::new();

    // 1. Send PairingRequest (Polo protocol_version = 2)
    let mut outer = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        pairing_request: Some(PairingRequest {
            service_name: "atvremote".to_string(),
            client_name: Some("Google TV Remote".to_string()),
        }),
        ..Default::default()
    };
    encode_frame(&outer, &mut write_buf)?;
    tls.write_all(&write_buf.split()).await?;

    // 2. Read PairingRequestAck
    let mut temp = [0u8; 1024];
    loop {
        let n = timeout(NETWORK_TIMEOUT, tls.read(&mut temp))
            .await
            .map_err(|_| PairingError::Timeout)??;
        if n == 0 {
            return Err(PairingError::Protocol(
                "Connection closed unexpectedly".to_string(),
            ));
        }
        read_buf.extend_from_slice(&temp[..n]);
        if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf)? {
            if msg.pairing_request_ack.is_some() {
                break;
            }
        }
    }

    // 3. Send Options
    outer = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        options: Some(Options {
            input_encodings: vec![options::Encoding {
                r#type: options::encoding::EncodingType::Hexadecimal as i32,
                symbol_length: 6,
            }],
            preferred_role: Some(options::RoleType::Input as i32),
            ..Default::default()
        }),
        ..Default::default()
    };
    encode_frame(&outer, &mut write_buf)?;
    tls.write_all(&write_buf.split()).await?;

    // 4. Read Configuration from TV
    loop {
        let n = timeout(NETWORK_TIMEOUT, tls.read(&mut temp))
            .await
            .map_err(|_| PairingError::Timeout)??;
        if n == 0 {
            return Err(PairingError::Protocol(
                "Connection closed waiting for config".to_string(),
            ));
        }
        read_buf.extend_from_slice(&temp[..n]);
        if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf)? {
            if msg.configuration.is_some() {
                break;
            }
        }
    }

    // 5. Send ConfigurationAck (TV displays the PIN on screen right after this)
    outer = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        configuration_ack: Some(ConfigurationAck {}),
        ..Default::default()
    };
    encode_frame(&outer, &mut write_buf)?;
    tls.write_all(&write_buf.split()).await?;

    // 6. Wait for PIN from user (up to 60 seconds)
    let pin = timeout(USER_PIN_TIMEOUT, pin_rx)
        .await
        .map_err(|_| PairingError::Timeout)?
        .map_err(|_| PairingError::ChannelClosed)?;

    let secret =
        calculate_pairing_secret(&client_mod, &client_exp, &server_mod, &server_exp, &pin)?;

    // 7. Send Secret
    outer = OuterMessage {
        protocol_version: 2,
        status: outer_message::Status::Ok as i32,
        secret: Some(Secret {
            secret: secret.to_vec(),
        }),
        ..Default::default()
    };
    encode_frame(&outer, &mut write_buf)?;
    tls.write_all(&write_buf.split()).await?;

    // 8. Read SecretAck
    loop {
        let n = timeout(NETWORK_TIMEOUT, tls.read(&mut temp))
            .await
            .map_err(|_| PairingError::Timeout)??;
        if n == 0 {
            return Err(PairingError::Protocol(
                "Connection closed waiting for secret ack".to_string(),
            ));
        }
        read_buf.extend_from_slice(&temp[..n]);
        if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf)? {
            if msg.status != outer_message::Status::Ok as i32 {
                return Err(PairingError::Protocol(format!(
                    "Pairing rejected with status: {}",
                    msg.status
                )));
            }
            if msg.secret_ack.is_some() {
                break;
            }
        }
    }

    Ok(())
}
