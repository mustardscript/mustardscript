use std::io::{self, BufRead, Read, Write};

use anyhow::{Context, Result};
use mustard_sidecar::{MAX_REQUEST_FRAME_BYTES, MAX_REQUEST_LINE_BYTES, SidecarSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportMode {
    Binary,
    Jsonl,
}

fn parse_transport_mode() -> Result<Option<TransportMode>> {
    let mut mode = TransportMode::Binary;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--jsonl" => mode = TransportMode::Jsonl,
            "--help" | "-h" => {
                println!("usage: mustard-sidecar [--jsonl]");
                println!("  default transport: length-prefixed binary frames");
                println!("  --jsonl: debug-friendly newline-delimited JSON transport");
                return Ok(None);
            }
            _ => anyhow::bail!("unknown argument `{arg}`; expected `--jsonl`"),
        }
    }
    Ok(Some(mode))
}

fn read_u32_or_eof<R: Read>(reader: &mut R) -> Result<Option<u32>> {
    let mut buf = [0u8; 4];
    let mut read = 0usize;
    while read < buf.len() {
        let count = reader
            .read(&mut buf[read..])
            .context("failed to read frame length")?;
        if count == 0 {
            if read == 0 {
                return Ok(None);
            }
            anyhow::bail!("unexpected EOF while reading frame length");
        }
        read += count;
    }
    Ok(Some(u32::from_le_bytes(buf)))
}

fn read_exact_vec<R: Read>(reader: &mut R, len: usize, context: &'static str) -> Result<Vec<u8>> {
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes).context(context)?;
    Ok(bytes)
}

fn run_binary_transport<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    session: &mut SidecarSession,
) -> Result<()> {
    loop {
        let Some(header_len) = read_u32_or_eof(stdin)? else {
            break;
        };
        let blob_len = read_u32_or_eof(stdin)?
            .ok_or_else(|| anyhow::anyhow!("unexpected EOF while reading blob length"))?;
        let total_len = header_len as usize + blob_len as usize;
        if total_len > MAX_REQUEST_FRAME_BYTES {
            anyhow::bail!("request frame exceeds maximum size of {MAX_REQUEST_FRAME_BYTES} bytes");
        }
        let header = read_exact_vec(
            stdin,
            header_len as usize,
            "failed to read request header bytes",
        )?;
        let blob = read_exact_vec(
            stdin,
            blob_len as usize,
            "failed to read request payload bytes",
        )?;
        let header_json =
            String::from_utf8(header).context("request header must be valid utf-8")?;
        let Some((response_header, response_blob)) =
            session.handle_request_frame(&header_json, &blob)?
        else {
            continue;
        };
        let response_header_len = u32::try_from(response_header.len())
            .context("response header exceeds binary transport length limit")?;
        let response_blob_len = u32::try_from(response_blob.len())
            .context("response payload exceeds binary transport length limit")?;
        stdout
            .write_all(&response_header_len.to_le_bytes())
            .context("failed to write response header length")?;
        stdout
            .write_all(&response_blob_len.to_le_bytes())
            .context("failed to write response payload length")?;
        stdout
            .write_all(&response_header)
            .context("failed to write response header")?;
        stdout
            .write_all(&response_blob)
            .context("failed to write response payload")?;
        stdout.flush().context("failed to flush response")?;
    }
    Ok(())
}

fn run_jsonl_transport<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    session: &mut SidecarSession,
) -> Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = stdin
            .read_until(b'\n', &mut line)
            .context("failed to read request line")?;
        if read == 0 {
            break;
        }
        if line.len() > MAX_REQUEST_LINE_BYTES + 1 {
            anyhow::bail!("request line exceeds maximum size of {MAX_REQUEST_LINE_BYTES} bytes");
        }
        if line.ends_with(b"\n") {
            line.pop();
            if line.ends_with(b"\r") {
                line.pop();
            }
        }
        let line = String::from_utf8(line.clone()).context("request line must be valid utf-8")?;
        let Some(response) = session.handle_request_line(&line)? else {
            continue;
        };
        writeln!(stdout, "{response}").context("failed to terminate response line")?;
        stdout.flush().context("failed to flush response")?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let Some(mode) = parse_transport_mode()? else {
        return Ok(());
    };
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut session = SidecarSession::new();
    match mode {
        TransportMode::Binary => run_binary_transport(&mut stdin, &mut stdout, &mut session),
        TransportMode::Jsonl => run_jsonl_transport(&mut stdin, &mut stdout, &mut session),
    }
}
