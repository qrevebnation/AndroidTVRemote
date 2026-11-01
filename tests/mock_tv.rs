use atv_remote::cert::generate_identity;
use atv_remote::codec::{decode_frame, encode_frame};
use atv_remote::pairing::{calculate_pairing_secret, extract_rsa_pubkey};
use atv_remote::proto::polo::*;
use bytes::BytesMut;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DistinguishedName, Error as RustlsError, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_rustls::{TlsAcceptor, TlsConnector};

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
async fn test_mock_tv_end_to_end_pairing() {
    let (server_cert, server_key) = generate_identity().unwrap();
    let (client_cert, client_key) = generate_identity().unwrap();

    let server_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(NoVerifyClientCert))
        .with_single_cert(vec![server_cert.clone()], server_key)
        .unwrap();

    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_cert_clone = server_cert.clone();
    let client_cert_clone = client_cert.clone();

    let (tv_pin_tx, tv_pin_rx) = oneshot::channel();

    // Spawn Mock TV Pairing Server
    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();

        let mut read_buf = BytesMut::new();
        let mut write_buf = BytesMut::new();
        let mut temp = [0u8; 1024];

        // 1. Read PairingRequest
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                assert!(msg.pairing_request.is_some());
                break;
            }
        }

        // 2. Send PairingRequestAck
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

        // 3. Read Options
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                assert!(msg.options.is_some());
                break;
            }
        }

        // 4. Send Configuration
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

        // 5. Read ConfigurationAck
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                assert!(msg.configuration_ack.is_some());
                break;
            }
        }

        // TV generates 6-char PIN:
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

        // Send PIN to client test (simulating user reading TV screen)
        tv_pin_tx.send(tv_displayed_pin).unwrap();

        // 6. Read Secret from client
        let secret_matched = loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
                if let Some(secret) = msg.secret {
                    break secret.secret == expected_secret;
                }
            }
        };
        assert!(secret_matched, "Client sent incorrect secret hash");

        // 7. Send SecretAck
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

    // Client side pairing execution
    let client_cert_saved = client_cert.clone();
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(MockVerifier))
        .with_client_auth_cert(vec![client_cert], client_key)
        .unwrap();
    config.alpn_protocols = vec![b"atvremote".to_vec()];

    let connector = TlsConnector::from(Arc::new(config));
    let tcp = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await.unwrap();
    let domain = ServerName::try_from("atvremote".to_string()).unwrap();
    let mut tls = connector.connect(domain, tcp).await.unwrap();

    let mut write_buf = BytesMut::new();
    let mut read_buf = BytesMut::new();
    let mut temp = [0u8; 1024];

    // Client sends PairingRequest
    let req = OuterMessage {
        protocol_version: 1,
        status: outer_message::Status::Ok as i32,
        pairing_request: Some(PairingRequest {
            service_name: "atvremote".to_string(),
            client_name: Some("Test Client".to_string()),
        }),
        ..Default::default()
    };
    encode_frame(&req, &mut write_buf).unwrap();
    tls.write_all(&write_buf.split()).await.unwrap();

    // Client reads PairingRequestAck
    loop {
        let n = tls.read(&mut temp).await.unwrap();
        read_buf.extend_from_slice(&temp[..n]);
        if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
            if msg.pairing_request_ack.is_some() {
                break;
            }
        }
    }

    // Client sends Options
    let opt = OuterMessage {
        protocol_version: 1,
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
    encode_frame(&opt, &mut write_buf).unwrap();
    tls.write_all(&write_buf.split()).await.unwrap();

    // Client reads Configuration
    loop {
        let n = tls.read(&mut temp).await.unwrap();
        read_buf.extend_from_slice(&temp[..n]);
        if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
            if msg.configuration.is_some() {
                break;
            }
        }
    }

    // Client sends ConfigurationAck
    let cfg_ack = OuterMessage {
        protocol_version: 1,
        status: outer_message::Status::Ok as i32,
        configuration_ack: Some(ConfigurationAck {}),
        ..Default::default()
    };
    encode_frame(&cfg_ack, &mut write_buf).unwrap();
    tls.write_all(&write_buf.split()).await.unwrap();

    // Receive PIN shown on TV screen
    let pin_val = tv_pin_rx.await.unwrap();

    // Client computes secret using its own cert (client_cert_saved) and peer cert (server)
    let (c_mod, c_exp) = extract_rsa_pubkey(client_cert_saved.as_ref()).unwrap();
    let (s_mod, s_exp) = extract_rsa_pubkey(
        tls.get_ref().1.peer_certificates().unwrap().first().unwrap().as_ref(),
    ).unwrap();

    let secret = calculate_pairing_secret(&c_mod, &c_exp, &s_mod, &s_exp, &pin_val).unwrap();

    // Client sends Secret
    let sec_msg = OuterMessage {
        protocol_version: 1,
        status: outer_message::Status::Ok as i32,
        secret: Some(Secret {
            secret: secret.to_vec(),
        }),
        ..Default::default()
    };
    encode_frame(&sec_msg, &mut write_buf).unwrap();
    tls.write_all(&write_buf.split()).await.unwrap();

    // Client reads SecretAck
    let secret_ack_received = loop {
        let n = tls.read(&mut temp).await.unwrap();
        read_buf.extend_from_slice(&temp[..n]);
        if let Some(msg) = decode_frame::<OuterMessage>(&mut read_buf).unwrap() {
            if msg.secret_ack.is_some() {
                break true;
            }
        }
    };
    assert!(secret_ack_received);

    server_handle.await.unwrap();
}

#[derive(Debug)]
struct MockVerifier;
impl rustls::client::danger::ServerCertVerifier for MockVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, RustlsError> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
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
