use bytes::BytesMut;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, Error as RustlsError, SignatureScheme};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_rustls::TlsConnector;

use crate::codec::{decode_frame, encode_frame};
pub use crate::proto::remote::{RemoteDirection, RemoteKeyCode};
use crate::proto::remote::*;

#[derive(Debug, Clone)]
pub enum Command {
    SendKey(RemoteKeyCode, RemoteDirection),
    LaunchApp(String),
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected(String),
    ActiveApp(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),
    #[error("Decode error: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("Protocol error: {0}")]
    Protocol(String),
}

#[derive(Debug)]
struct NoVerifyServerCert;

impl ServerCertVerifier for NoVerifyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
        ]
    }
}

pub async fn run_remote_session(
    tv_ip: &str,
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    cmd_rx: mpsc::UnboundedReceiver<Command>,
    status_tx: mpsc::UnboundedSender<ConnectionState>,
) -> Result<(), RemoteError> {
    run_remote_session_with_port(tv_ip, 6467, cert, key, cmd_rx, status_tx).await
}

pub async fn run_remote_session_with_port(
    tv_ip: &str,
    port: u16,
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    status_tx: mpsc::UnboundedSender<ConnectionState>,
) -> Result<(), RemoteError> {
    let _ = status_tx.send(ConnectionState::Connecting);

    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifyServerCert))
        .with_client_auth_cert(vec![cert], key)?;

    config.alpn_protocols = vec![b"atvremote".to_vec()];

    let connector = TlsConnector::from(Arc::new(config));
    let addr = format!("{}:{}", tv_ip, port);
    let tcp = TcpStream::connect(&addr).await?;
    let domain = ServerName::try_from("atvremote".to_string())
        .map_err(|_| RemoteError::Protocol("Invalid server name".to_string()))?;

    let mut tls = connector.connect(domain, tcp).await?;

    let mut write_buf = BytesMut::new();
    let mut read_buf = BytesMut::new();

    // 1. Send RemoteConfigure
    let configure_msg = RemoteMessage {
        remote_configure: Some(RemoteConfigure {
            code1: 1,
            device_info: Some(RemoteDeviceInfo {
                model: "Desktop Remote".to_string(),
                vendor: "OpenSource".to_string(),
                unknown1: 1,
                unknown2: "1".to_string(),
                package_name: "atv.remote".to_string(),
                app_version: "1.0.0".to_string(),
            }),
        }),
        ..Default::default()
    };
    encode_frame(&configure_msg, &mut write_buf)?;
    tls.write_all(&write_buf.split()).await?;

    let _ = status_tx.send(ConnectionState::Connected);

    let mut temp = [0u8; 2048];

    loop {
        tokio::select! {
            read_res = tls.read(&mut temp) => {
                let n = match read_res {
                    Ok(0) => {
                        let _ = status_tx.send(ConnectionState::Disconnected("Connection closed by TV".to_string()));
                        break;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                        return Err(RemoteError::Io(e));
                    }
                };

                read_buf.extend_from_slice(&temp[..n]);

                while let Some(msg) = decode_frame::<RemoteMessage>(&mut read_buf)? {
                    // Handle Ping/Pong
                    if let Some(ping) = msg.remote_ping_request {
                        let pong = RemoteMessage {
                            remote_ping_response: Some(RemotePingResponse { val1: ping.val1 }),
                            ..Default::default()
                        };
                        encode_frame(&pong, &mut write_buf)?;
                        tls.write_all(&write_buf.split()).await?;
                    }
                }
            }

            cmd_opt = cmd_rx.recv() => {
                match cmd_opt {
                    Some(Command::SendKey(key_code, direction)) => {
                        let key_msg = RemoteMessage {
                            remote_key_inject: Some(RemoteKeyInject {
                                key_code: key_code as i32,
                                direction: direction as i32,
                            }),
                            ..Default::default()
                        };
                        encode_frame(&key_msg, &mut write_buf)?;
                        tls.write_all(&write_buf.split()).await?;
                    }
                    Some(Command::LaunchApp(url)) => {
                        let app_msg = RemoteMessage {
                            remote_app_link_launch_request: Some(RemoteAppLinkLaunchRequest {
                                app_link: url,
                            }),
                            ..Default::default()
                        };
                        encode_frame(&app_msg, &mut write_buf)?;
                        tls.write_all(&write_buf.split()).await?;
                    }
                    Some(Command::Disconnect) | None => {
                        let _ = status_tx.send(ConnectionState::Disconnected("Disconnected by user".to_string()));
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
