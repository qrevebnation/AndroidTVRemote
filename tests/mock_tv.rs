use atv_remote::cert::generate_identity;
use atv_remote::codec::{decode_frame, encode_frame};
use atv_remote::pairing::{extract_rsa_pubkey, start_pairing_with_port};
use atv_remote::proto::polo::*;
use atv_remote::proto::remote::{
    RemoteDirection, RemoteKeyCode, RemoteMessage as ActiveRemoteMessage, RemoteSetActive,
};
use atv_remote::remote::{run_remote_session_with_port, Command, ConnectionState};
use bytes::BytesMut;
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DistinguishedName, Error as RustlsError, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_rustls::TlsAcceptor;

#[derive(Debug)]
struct NoVerifyClientCert;

impl ClientCertVerifier for NoVerifyClientCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, RustlsError> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, RustlsError> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[tokio::test]
async fn test_mock_tv_end_to_end_pairing_and_remote_control() {
    let (server_cert, server_key) = generate_identity().unwrap();
    let server_key_clone = server_key.clone_key();
    let (client_cert, client_key) = generate_identity().unwrap();

    let server_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(NoVerifyClientCert))
        .with_single_cert(vec![server_cert.clone()], server_key)
        .unwrap();

    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let pairing_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pairing_port = pairing_listener.local_addr().unwrap().port();

    let remote_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let remote_port = remote_listener.local_addr().unwrap().port();

    let server_cert_clone = server_cert.clone();
    let client_cert_clone = client_cert.clone();

    let (tv_pin_tx, tv_pin_rx) = oneshot::channel();

    // 1. Spawn Mock TV Pairing Server (Port 6466 simulator)
    let pairing_server = tokio::spawn(async move {
        let (stream, _) = pairing_listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();

        let mut read_buf = BytesMut::new();
        let mut write_buf = BytesMut::new();
        let mut temp = [0u8; 1024];

        // Read PairingRequest
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                assert!(msg.pairing_request.is_some());
                break;
            }
        }

        // Send PairingRequestAck
        let mut resp = OuterMessage {
            protocol_version: 1,
            status: outer_message::Status::Ok as i32,
            pairing_request_ack: Some(PairingRequestAck {
                server_name: Some("Mock Android TV".to_string()),
            }),
            ..Default::default()
        };
        encode_frame(&resp, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();

        // Read Options
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                assert!(msg.options.is_some());
                break;
            }
        }

        // Send Configuration
        resp = OuterMessage {
            protocol_version: 1,
            status: outer_message::Status::Ok as i32,
            configuration: Some(Configuration {
                encoding: options::Encoding {
                    r#type: options::encoding::EncodingType::Hexadecimal as i32,
                    symbol_length: 6,
                },
                client_role: options::RoleType::Input as i32,
            }),
            ..Default::default()
        };
        encode_frame(&resp, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();

        // Read ConfigurationAck
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                assert!(msg.configuration_ack.is_some());
                break;
            }
        }

        // TV generates PIN:
        let (c_mod, c_exp) = extract_rsa_pubkey(client_cert_clone.as_ref()).unwrap();
        let (s_mod, s_exp) = extract_rsa_pubkey(server_cert_clone.as_ref()).unwrap();

        let pin_tail = "8F1C";
        let mut hasher = Sha256::new();
        hasher.update(&c_mod);
        hasher.update(&c_exp);
        hasher.update(&s_mod);
        hasher.update(&s_exp);
        hasher.update(hex::decode(pin_tail).unwrap());
        let hash_res = hasher.finalize();
        let tv_displayed_pin = format!("{:02X}{}", hash_res[0], pin_tail);
        let expected_secret: [u8; 32] = hash_res.into();

        tv_pin_tx.send(tv_displayed_pin).unwrap();

        // Read Secret from client
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                if let Some(secret) = msg.secret {
                    assert_eq!(secret.secret, expected_secret);
                    break;
                }
            }
        }

        // Send SecretAck
        resp = OuterMessage {
            protocol_version: 1,
            status: outer_message::Status::Ok as i32,
            secret_ack: Some(SecretAck {
                secret: expected_secret.to_vec(),
            }),
            ..Default::default()
        };
        encode_frame(&resp, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();
    });

    // Run real production start_pairing_with_port!
    let client_cert_for_remote = client_cert.clone();
    let client_key_for_remote = client_key.clone_key();

    let client_pairing_task = tokio::spawn(async move {
        start_pairing_with_port(
            "127.0.0.1",
            pairing_port,
            &client_cert,
            &client_key,
            tv_pin_rx,
        )
        .await
        .expect("Production start_pairing_with_port failed");
    });

    client_pairing_task.await.unwrap();
    pairing_server.await.unwrap();

    // 2. Now verify real Remote session key delivery on port 6467
    let remote_acceptor = TlsAcceptor::from(Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![server_cert], server_key_clone)
            .unwrap(),
    ));

    let remote_server = tokio::spawn(async move {
        let (stream, _) = remote_listener.accept().await.unwrap();
        let mut tls = remote_acceptor.accept(stream).await.unwrap();

        let mut read_buf = BytesMut::new();
        let mut write_buf = BytesMut::new();
        let mut temp = [0u8; 1024];

        // Read RemoteConfigure
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<ActiveRemoteMessage>(&mut read_buf).unwrap() {
                if msg.remote_configure.is_some() {
                    break;
                }
            }
        }

        // Acknowledge with RemoteSetActive
        let resp = ActiveRemoteMessage {
            remote_set_active: Some(RemoteSetActive { active: 1 }),
            ..Default::default()
        };
        encode_frame(&resp, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();

        // Read Key Inject
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<ActiveRemoteMessage>(&mut read_buf).unwrap() {
                if let Some(key) = msg.remote_key_inject {
                    assert_eq!(key.key_code, RemoteKeyCode::KeycodeHome as i32);
                    break;
                }
            }
        }
    });

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (status_tx, mut status_rx) = mpsc::unbounded_channel();

    let client_remote_task = tokio::spawn(async move {
        run_remote_session_with_port(
            "127.0.0.1",
            remote_port,
            client_cert_for_remote,
            client_key_for_remote,
            cmd_rx,
            status_tx,
        )
        .await
        .unwrap();
    });

    while let Some(state) = status_rx.recv().await {
        if state == ConnectionState::Connected {
            break;
        }
    }

    cmd_tx
        .send(Command::SendKey(
            RemoteKeyCode::KeycodeHome,
            RemoteDirection::Short,
        ))
        .unwrap();

    remote_server.await.unwrap();
    cmd_tx.send(Command::Disconnect).unwrap();
    let _ = client_remote_task.await;
}
