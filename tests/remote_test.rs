use atv_remote::cert::generate_identity;
use atv_remote::codec::{decode_frame, encode_frame};
use atv_remote::proto::remote::{
    RemoteConfigure, RemoteDeviceInfo, RemoteDirection, RemoteKeyCode, RemoteMessage,
    RemoteSetActive, RemoteStart,
};
use atv_remote::remote::{run_remote_session_with_port, Command, ConnectionState};
use bytes::BytesMut;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;

#[tokio::test]
async fn test_remote_session_handshake_and_key_injection() {
    let (server_cert, server_key) = generate_identity().unwrap();
    let (client_cert, client_key) = generate_identity().unwrap();

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![server_cert], server_key)
        .unwrap();

    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (status_tx, mut status_rx) = mpsc::unbounded_channel();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();

        let mut read_buf = BytesMut::new();
        let mut write_buf = BytesMut::new();
        let mut temp = [0u8; 1024];

        // 1. Server sends RemoteConfigure to client
        let cfg = RemoteMessage {
            remote_configure: Some(RemoteConfigure {
                code1: 639,
                device_info: Some(RemoteDeviceInfo {
                    model: "TV".to_string(),
                    vendor: "Vendor".to_string(),
                    unknown1: 1,
                    unknown2: "1".to_string(),
                    package_name: "tv.app".to_string(),
                    app_version: "1.0".to_string(),
                }),
            }),
            ..Default::default()
        };
        encode_frame(&cfg, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();
        tls.flush().await.unwrap();

        // 2. Read client RemoteConfigure response
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<RemoteMessage>(&mut read_buf).unwrap() {
                if msg.remote_configure.is_some() {
                    break;
                }
            }
        }

        // 3. Server sends RemoteSetActive
        let act = RemoteMessage {
            remote_set_active: Some(RemoteSetActive { active: 639 }),
            ..Default::default()
        };
        encode_frame(&act, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();
        tls.flush().await.unwrap();

        // 4. Read client RemoteSetActive response
        loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<RemoteMessage>(&mut read_buf).unwrap() {
                if msg.remote_set_active.is_some() {
                    break;
                }
            }
        }

        // 5. Server sends RemoteStart
        let start = RemoteMessage {
            remote_start: Some(RemoteStart { started: true }),
            ..Default::default()
        };
        encode_frame(&start, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();
        tls.flush().await.unwrap();

        // 6. Read RemoteKeyInject from client
        let key_received = loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<RemoteMessage>(&mut read_buf).unwrap() {
                if let Some(key_inject) = msg.remote_key_inject {
                    break key_inject.key_code == RemoteKeyCode::KeycodeDpadCenter as i32;
                }
            }
        };
        assert!(key_received);
    });

    let client_task = tokio::spawn(async move {
        run_remote_session_with_port(
            "127.0.0.1",
            port,
            client_cert,
            client_key,
            cmd_rx,
            status_tx,
        )
        .await
        .unwrap();
    });

    while let Some(state) = status_rx.recv().await {
        if let ConnectionState::Connected = state {
            break;
        }
    }

    cmd_tx
        .send(Command::SendKey(
            RemoteKeyCode::KeycodeDpadCenter,
            RemoteDirection::Short,
        ))
        .unwrap();

    server_task.await.unwrap();
    cmd_tx.send(Command::Disconnect).unwrap();
    let _ = client_task.await;
}
