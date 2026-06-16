//! Long-lived bridge server for server-side use.
//!
//! Reads newline-delimited `BridgeRequest` JSON on stdin and writes the
//! corresponding newline-delimited `BridgeResponse` JSON on stdout, one
//! response per request, in order. This reuses the exact same
//! `handle_bridge_request_json` dispatch the on-device bridge uses, so the
//! server drives `bull-core` through identical methods.
//!
//! Protocol: one JSON object per line in, one JSON object per line out.
//! A blank line is ignored. EOF on stdin ends the process cleanly.

use std::io::{self, BufRead, Write};

use bull_core::bridge::handle_bridge_request_json;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("bull-bridge-serve: stdin read error: {error}");
                std::process::exit(2);
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = handle_bridge_request_json(trimmed);
        // One response per line; flush so the caller can read it immediately.
        if let Err(error) = writeln!(out, "{response}").and_then(|_| out.flush()) {
            eprintln!("bull-bridge-serve: stdout write error: {error}");
            std::process::exit(2);
        }
    }
}
