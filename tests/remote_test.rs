use atv_remote::cert::generate_identity;
use atv_remote::codec::{decode_frame, encode_frame};
use atv_remote::proto::remote::{
    RemoteDirection, RemoteKeyCode, RemoteMessage, RemoteSetActive,
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
    // Generate server cert
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

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();

        let mut read_buf = BytesMut::new();
        let mut write_buf = BytesMut::new();
        let mut temp = [0u8; 1024];

        // 1. Read RemoteConfigure from client
        let configure_received = loop {
            let n = tls.read(&mut temp).await.unwrap();
            read_buf.extend_from_slice(&temp[..n]);
            if let Some(msg) = decode_frame::<RemoteMessage>(&mut read_buf).unwrap() {
                if msg.remote_configure.is_some() {
                    break true;
                }
            }
        };
        assert!(configure_received);

        // 2. Respond with RemoteSetActive
        let resp = RemoteMessage {
            remote_set_active: Some(RemoteSetActive { active: 1 }),
            ..Default::default()
        };
        encode_frame(&resp, &mut write_buf).unwrap();
        tls.write_all(&write_buf.split()).await.unwrap();

        // 3. Read RemoteKeyInject from client
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

    // Spawn client task
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

    // Wait for Connected status
    while let Some(state) = status_rx.recv().await {
        if let ConnectionState::Connected = state {
            break;
        }
    }

    // Send key command
    cmd_tx
        .send(Command::SendKey(
            RemoteKeyCode::KeycodeDpadCenter,
            RemoteDirection::Short,
        ))
        .unwrap();

    // Wait for server to verify
    server_task.await.unwrap();

    // Send disconnect
    cmd_tx.send(Command::Disconnect).unwrap();
    let _ = client_task.await;
}
