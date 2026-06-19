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

/// Pull `method` and `request_id` out of a request line without a full parse,
/// so a crash log can name the in-flight request even for huge payloads.
fn request_label(line: &str) -> String {
    let field = |key: &str| -> Option<&str> {
        let needle = format!("\"{key}\"");
        let start = line.find(&needle)? + needle.len();
        let rest = line[start..].trim_start();
        let rest = rest.strip_prefix(':')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let end = rest.find('"')?;
        Some(&rest[..end])
    };
    format!(
        "method={} request_id={} bytes={}",
        field("method").unwrap_or("?"),
        field("request_id").unwrap_or("?"),
        line.len()
    )
}

fn main() {
    // Surface panics with location + payload on stderr (captured by the host).
    // catch_unwind in the bridge turns panics into error responses, but logging
    // here guarantees the cause is recorded even if a panic escapes a thread.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("bull-bridge-serve: PANIC {info}");
    }));

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
        // Log the in-flight request BEFORE dispatch. A hard crash (segfault /
        // stack overflow / abort) runs no panic hook, so this is the only
        // breadcrumb identifying which request killed the process: it will be
        // the last line on stderr.
        eprintln!("bull-bridge-serve: handling {}", request_label(trimmed));
        let response = handle_bridge_request_json(trimmed);
        // One response per line; flush so the caller can read it immediately.
        if let Err(error) = writeln!(out, "{response}").and_then(|_| out.flush()) {
            eprintln!("bull-bridge-serve: stdout write error: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::request_label;

    #[test]
    fn labels_extract_method_and_request_id() {
        let line = r#"{"schema":"bull.bridge.request.v1","request_id":"42","method":"capture.import_frame_batch","args":{}}"#;
        let label = request_label(line);
        assert!(label.contains("method=capture.import_frame_batch"), "{label}");
        assert!(label.contains("request_id=42"), "{label}");
        assert!(label.contains(&format!("bytes={}", line.len())), "{label}");
    }

    #[test]
    fn labels_degrade_gracefully_on_garbage() {
        let label = request_label("not json at all");
        assert!(label.contains("method=?"));
        assert!(label.contains("request_id=?"));
    }
}
