use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use jslite_sidecar::handle_request_line;

fn main() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.context("failed to read request line")?;
        let Some(response) = handle_request_line(&line)? else {
            continue;
        };
        writeln!(&mut stdout, "{response}").context("failed to terminate response line")?;
        stdout.flush().context("failed to flush response")?;
    }
    Ok(())
}
