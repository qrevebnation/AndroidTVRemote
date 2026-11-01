use atv_remote::proto::{polo, remote};
use prost::Message;

#[test]
fn test_polo_message_serialization() {
    let mut msg = polo::OuterMessage::default();
    msg.protocol_version = 1;
    msg.status = polo::outer_message::Status::Ok as i32;

    let mut req = polo::PairingRequest::default();
    req.service_name = "atvremote".to_string();
    req.client_name = Some("Linux Remote".to_string());
    msg.pairing_request = Some(req);

    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("encode failed");
    assert!(!buf.is_empty());

    let decoded = polo::OuterMessage::decode(&buf[..]).expect("decode failed");
    assert_eq!(decoded.status, polo::outer_message::Status::Ok as i32);
    let inner_req = decoded.pairing_request.expect("missing pairing request");
    assert_eq!(inner_req.service_name, "atvremote");
    assert_eq!(inner_req.client_name.as_deref(), Some("Linux Remote"));
}

#[test]
fn test_remote_key_inject_serialization() {
    let mut msg = remote::RemoteMessage::default();
    msg.remote_key_inject = Some(remote::RemoteKeyInject {
        key_code: remote::RemoteKeyCode::KeycodeDpadCenter as i32,
        direction: remote::RemoteDirection::Short as i32,
    });

    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("encode failed");
    assert!(!buf.is_empty());

    let decoded = remote::RemoteMessage::decode(&buf[..]).expect("decode failed");
    let key_inject = decoded.remote_key_inject.expect("missing key inject");
    assert_eq!(key_inject.key_code, remote::RemoteKeyCode::KeycodeDpadCenter as i32);
    assert_eq!(key_inject.direction, remote::RemoteDirection::Short as i32);
}
