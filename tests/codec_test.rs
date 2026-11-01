use atv_remote::codec::{decode_frame, encode_frame};
use atv_remote::proto::remote::{RemoteDirection, RemoteKeyCode, RemoteKeyInject, RemoteMessage};
use bytes::BytesMut;

#[test]
fn test_encode_and_decode_single_frame() {
    let mut msg = RemoteMessage::default();
    msg.remote_key_inject = Some(RemoteKeyInject {
        key_code: RemoteKeyCode::KeycodeDpadUp as i32,
        direction: RemoteDirection::Short as i32,
    });

    let mut buf = BytesMut::new();
    encode_frame(&msg, &mut buf).expect("encode_frame failed");
    assert!(!buf.is_empty());

    let decoded: Option<RemoteMessage> = decode_frame(&mut buf).expect("decode_frame failed");
    assert!(decoded.is_some());
    let unwrapped = decoded.unwrap();
    assert_eq!(
        unwrapped.remote_key_inject.unwrap().key_code,
        RemoteKeyCode::KeycodeDpadUp as i32
    );
    assert!(buf.is_empty());
}

#[test]
fn test_decode_partial_frame_returns_none() {
    let mut msg = RemoteMessage::default();
    msg.remote_key_inject = Some(RemoteKeyInject {
        key_code: RemoteKeyCode::KeycodeBack as i32,
        direction: RemoteDirection::Short as i32,
    });

    let mut full_buf = BytesMut::new();
    encode_frame(&msg, &mut full_buf).expect("encode failed");

    // Feed only 2 bytes
    let mut partial = BytesMut::from(&full_buf[..2]);
    let result: Option<RemoteMessage> = decode_frame(&mut partial).expect("partial decode failed");
    assert!(result.is_none());
    assert_eq!(partial.len(), 2); // Buffer must not be consumed
}

#[test]
fn test_decode_multiple_frames() {
    let mut msg1 = RemoteMessage::default();
    msg1.remote_key_inject = Some(RemoteKeyInject {
        key_code: RemoteKeyCode::KeycodeVolumeUp as i32,
        direction: RemoteDirection::Short as i32,
    });

    let mut msg2 = RemoteMessage::default();
    msg2.remote_key_inject = Some(RemoteKeyInject {
        key_code: RemoteKeyCode::KeycodeVolumeDown as i32,
        direction: RemoteDirection::Short as i32,
    });

    let mut buf = BytesMut::new();
    encode_frame(&msg1, &mut buf).expect("encode msg1 failed");
    encode_frame(&msg2, &mut buf).expect("encode msg2 failed");

    let dec1: Option<RemoteMessage> = decode_frame(&mut buf).expect("decode 1 failed");
    assert_eq!(
        dec1.unwrap().remote_key_inject.unwrap().key_code,
        RemoteKeyCode::KeycodeVolumeUp as i32
    );

    let dec2: Option<RemoteMessage> = decode_frame(&mut buf).expect("decode 2 failed");
    assert_eq!(
        dec2.unwrap().remote_key_inject.unwrap().key_code,
        RemoteKeyCode::KeycodeVolumeDown as i32
    );

    let dec3: Option<RemoteMessage> = decode_frame(&mut buf).expect("decode 3 failed");
    assert!(dec3.is_none());
    assert!(buf.is_empty());
}
