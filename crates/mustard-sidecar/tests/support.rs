#![allow(dead_code)]

use std::io::{Read, Write};

use hmac::{Hmac, Mac};
use mustard_sidecar::SidecarSession;
use serde_json::Value;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

pub fn snapshot_key_digest(snapshot_key: &[u8]) -> String {
    digest_hex(snapshot_key)
}

pub fn snapshot_token(snapshot: &[u8], snapshot_key: &[u8]) -> String {
    let snapshot_id = digest_hex(snapshot);
    let mut mac = HmacSha256::new_from_slice(snapshot_key).expect("snapshot key should be valid");
    mac.update(snapshot_id.as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut token = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

pub fn request_binary(
    session: &mut SidecarSession,
    header: Value,
    blob: &[u8],
) -> (Value, Vec<u8>) {
    let header_json = header.to_string();
    let Some((response_header, response_blob)) = session
        .handle_request_frame(&header_json, blob)
        .unwrap_or_else(|error| panic!("request should succeed:\n{header_json}\n{error}"))
    else {
        panic!("request should yield a response frame");
    };
    let response: Value =
        serde_json::from_slice(&response_header).expect("response header should parse");
    (response, response_blob)
}

pub fn write_binary_frame<W: Write>(writer: &mut W, header: Value, blob: &[u8]) {
    let header_json = header.to_string();
    let header_bytes = header_json.as_bytes();
    writer
        .write_all(&(header_bytes.len() as u32).to_le_bytes())
        .expect("frame header length should write");
    writer
        .write_all(&(blob.len() as u32).to_le_bytes())
        .expect("frame payload length should write");
    writer
        .write_all(header_bytes)
        .expect("frame header should write");
    writer.write_all(blob).expect("frame payload should write");
    writer.flush().expect("frame should flush");
}

pub fn read_binary_frame<R: Read>(reader: &mut R) -> (Value, Vec<u8>) {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .expect("response header length should read");
    let header_len = u32::from_le_bytes(len_buf) as usize;
    reader
        .read_exact(&mut len_buf)
        .expect("response payload length should read");
    let blob_len = u32::from_le_bytes(len_buf) as usize;
    let mut header = vec![0u8; header_len];
    reader
        .read_exact(&mut header)
        .expect("response header should read");
    let mut blob = vec![0u8; blob_len];
    reader
        .read_exact(&mut blob)
        .expect("response payload should read");
    (
        serde_json::from_slice(&header).expect("response header should parse"),
        blob,
    )
}
