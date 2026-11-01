use bytes::{Buf, BytesMut};
use prost::encoding::{decode_varint, encode_varint, encoded_len_varint};
use prost::{DecodeError, Message};

/// Encodes a protobuf message with a varint length prefix into the given buffer.
pub fn encode_frame<M: Message>(msg: &M, buf: &mut BytesMut) -> Result<(), std::io::Error> {
    let len = msg.encoded_len();
    buf.reserve(encoded_len_varint(len as u64) + len);
    encode_varint(len as u64, buf);
    msg.encode(buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(())
}

/// Decodes a length-delimited protobuf message from the buffer if complete.
/// If incomplete, returns `Ok(None)` and leaves the buffer intact.
pub fn decode_frame<M: Message + Default>(buf: &mut BytesMut) -> Result<Option<M>, DecodeError> {
    if buf.is_empty() {
        return Ok(None);
    }

    let mut peek_slice = &buf[..];
    let initial_len = peek_slice.len();
    let msg_len = match decode_varint(&mut peek_slice) {
        Ok(len) => len as usize,
        Err(_) => {
            // Varint prefix not fully received yet
            return Ok(None);
        }
    };

    let varint_len = initial_len - peek_slice.len();
    let total_frame_len = varint_len + msg_len;

    if buf.len() < total_frame_len {
        // Complete payload not yet available in buffer
        return Ok(None);
    }

    buf.advance(varint_len);
    let msg_bytes = buf.split_to(msg_len);
    let msg = M::decode(msg_bytes)?;
    Ok(Some(msg))
}
