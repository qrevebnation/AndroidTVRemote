use bytes::BytesMut;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::ClientConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use crate::cert::NoVerifyServerCert;
use crate::codec::{decode_frame, encode_frame};
use crate::proto::remote::*;
pub use crate::proto::remote::{RemoteDirection, RemoteKeyCode};

const NETWORK_TIMEOUT: Duration = Duration::from_secs(7);

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
    #[error("Operation timed out")]
    Timeout,
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
    let tcp = timeout(NETWORK_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| RemoteError::Timeout)??;
    let domain = ServerName::try_from("atvremote".to_string())
        .map_err(|_| RemoteError::Protocol("Invalid server name".to_string()))?;

    let mut tls = timeout(NETWORK_TIMEOUT, connector.connect(domain, tcp))
        .await
        .map_err(|_| RemoteError::Timeout)??;

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

    // 2. Wait for confirmation before claiming Connected
    let mut temp = [0u8; 2048];
    let mut handshake_confirmed = false;
    while !handshake_confirmed {
        let n = timeout(NETWORK_TIMEOUT, tls.read(&mut temp))
            .await
            .map_err(|_| RemoteError::Timeout)??;
        if n == 0 {
            let _ = status_tx.send(ConnectionState::Disconnected(
                "Connection closed by TV".to_string(),
            ));
            return Err(RemoteError::Protocol(
                "Connection closed before configuration ack".to_string(),
            ));
        }
        read_buf.extend_from_slice(&temp[..n]);
        while let Some(msg) = decode_frame::<RemoteMessage>(&mut read_buf)? {
            if msg.remote_configure.is_some() || msg.remote_set_active.is_some() {
                handshake_confirmed = true;
                break;
            }
        }
    }

    let _ = status_tx.send(ConnectionState::Connected);

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

                loop {
                    match decode_frame::<RemoteMessage>(&mut read_buf) {
                        Ok(Some(msg)) => {
                            if let Some(ping) = msg.remote_ping_request {
                                let pong = RemoteMessage {
                                    remote_ping_response: Some(RemotePingResponse { val1: ping.val1 }),
                                    ..Default::default()
                                };
                                if let Err(e) = encode_frame(&pong, &mut write_buf) {
                                    let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                                    return Err(RemoteError::Io(e));
                                }
                                if let Err(e) = tls.write_all(&write_buf.split()).await {
                                    let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                                    return Err(RemoteError::Io(e));
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = status_tx.send(ConnectionState::Disconnected(format!("Frame decode error: {e}")));
                            return Err(RemoteError::Decode(e));
                        }
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
                        if let Err(e) = encode_frame(&key_msg, &mut write_buf) {
                            let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                            return Err(RemoteError::Io(e));
                        }
                        if let Err(e) = tls.write_all(&write_buf.split()).await {
                            let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                            return Err(RemoteError::Io(e));
                        }
                    }
                    Some(Command::LaunchApp(url)) => {
                        let app_msg = RemoteMessage {
                            remote_app_link_launch_request: Some(RemoteAppLinkLaunchRequest {
                                app_link: url,
                            }),
                            ..Default::default()
                        };
                        if let Err(e) = encode_frame(&app_msg, &mut write_buf) {
                            let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                            return Err(RemoteError::Io(e));
                        }
                        if let Err(e) = tls.write_all(&write_buf.split()).await {
                            let _ = status_tx.send(ConnectionState::Disconnected(e.to_string()));
                            return Err(RemoteError::Io(e));
                        }
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
