use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use mustard_sidecar::{MAX_REQUEST_LINE_BYTES, SidecarSession};

fn main() -> Result<()> {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut session = SidecarSession::new();
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
        writeln!(&mut stdout, "{response}").context("failed to terminate response line")?;
        stdout.flush().context("failed to flush response")?;
    }
    Ok(())
}
