use std::{
    net::{TcpListener, ToSocketAddrs},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tungstenite::{
    Message, accept_hdr,
    handshake::server::{ErrorResponse, Request, Response},
    http::StatusCode,
};

use crate::{
    BullError, BullResult,
    debug_ws::{DebugBridgeConfig, debug_bridge_config_issues, debug_event_envelope_from_row},
    store::BullStore,
};

#[derive(Debug, Clone)]
pub struct DebugWsServerOptions {
    pub database_path: PathBuf,
    pub session_id: String,
    pub bind_host: String,
    pub port: u16,
    pub token: String,
    pub poll_interval_ms: u64,
    pub idle_timeout_ms: u64,
    pub max_events: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugWsServeReport {
    pub schema: String,
    pub generated_by: String,
    pub bind_url: String,
    pub session_id: String,
    pub pass: bool,
    #[serde(default)]
    pub server_valid: bool,
    #[serde(default)]
    pub handshake_accepted: bool,
    #[serde(default)]
    pub session_found: bool,
    #[serde(default)]
    pub stream_observed: bool,
    pub completion_reason: String,
    pub client_count: usize,
    pub events_sent: usize,
    pub last_sequence: u64,
    pub issues: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<DebugWsServeNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugWsServeNextAction {
    pub scope: String,
    pub reason: String,
    pub action: String,
}

pub fn serve_debug_ws_once(options: DebugWsServerOptions) -> BullResult<DebugWsServeReport> {
    let listener = bind_debug_ws_listener(&options)?;
    serve_debug_ws_listener_once(listener, options)
}

pub fn bind_debug_ws_listener(options: &DebugWsServerOptions) -> BullResult<TcpListener> {
    validate_server_options(options)?;
    let mut addresses = (options.bind_host.as_str(), options.port)
        .to_socket_addrs()
        .map_err(|error| {
            BullError::message(format!("cannot resolve debug bind address: {error}"))
        })?;
    let address = addresses
        .next()
        .ok_or_else(|| BullError::message("debug bind address did not resolve"))?;
    TcpListener::bind(address)
        .map_err(|error| BullError::io(format!("{}:{}", options.bind_host, options.port), error))
}

pub fn serve_debug_ws_listener_once(
    listener: TcpListener,
    options: DebugWsServerOptions,
) -> BullResult<DebugWsServeReport> {
    validate_server_options(&options)?;
    let bind_url = debug_ws_url(&listener, &options)?;
    let (stream, _) = listener
        .accept()
        .map_err(|error| BullError::io(&options.database_path, error))?;

    let token = options.token.clone();
    let callback = move |request: &Request, response: Response| {
        validate_handshake_request(request, &token).map(|()| response)
    };
    let mut websocket = match accept_hdr(stream, callback) {
        Ok(websocket) => websocket,
        Err(error) => {
            return Ok(report(
                bind_url,
                options.session_id,
                "handshake_failed",
                1,
                0,
                0,
                vec![format!("websocket handshake failed: {error}")],
                false,
                false,
            ));
        }
    };

    let store = BullStore::open(&options.database_path)?;
    if store.debug_session(&options.session_id)?.is_none() {
        return Ok(report(
            bind_url,
            options.session_id,
            "session_not_found",
            1,
            0,
            0,
            vec!["debug_session_not_found".to_string()],
            true,
            false,
        ));
    }

    let poll_interval = Duration::from_millis(options.poll_interval_ms);
    let idle_timeout = Duration::from_millis(options.idle_timeout_ms);
    let mut last_sequence = 0_u64;
    let mut events_sent = 0_usize;
    let mut last_activity = Instant::now();

    loop {
        let rows = store.debug_events_after_sequence(
            &options.session_id,
            i64::try_from(last_sequence)
                .map_err(|_| BullError::message("last_sequence is too large"))?,
            Some(100),
        )?;
        if rows.is_empty() {
            if last_activity.elapsed() >= idle_timeout {
                let _ = websocket.close(None);
                return Ok(report(
                    bind_url,
                    options.session_id,
                    "idle_timeout",
                    1,
                    events_sent,
                    last_sequence,
                    Vec::new(),
                    true,
                    true,
                ));
            }
            thread::sleep(poll_interval);
            continue;
        }

        for row in rows {
            let event = debug_event_envelope_from_row(row)?;
            last_sequence = event.sequence;
            let event_json = serde_json::to_string(&event).map_err(|error| {
                BullError::message(format!("cannot serialize debug event: {error}"))
            })?;
            websocket
                .send(Message::Text(event_json.into()))
                .map_err(|error| {
                    BullError::message(format!("cannot write websocket event: {error}"))
                })?;
            events_sent += 1;
            last_activity = Instant::now();

            if options
                .max_events
                .is_some_and(|max_events| events_sent >= max_events)
            {
                let _ = websocket.close(None);
                return Ok(report(
                    bind_url,
                    options.session_id,
                    "max_events_reached",
                    1,
                    events_sent,
                    last_sequence,
                    Vec::new(),
                    true,
                    true,
                ));
            }
        }
    }
}

fn validate_server_options(options: &DebugWsServerOptions) -> BullResult<()> {
    if options.session_id.trim().is_empty() {
        return Err(BullError::message("session_id is required"));
    }
    if options.token.trim().is_empty() {
        return Err(BullError::message("token is required"));
    }
    if options.poll_interval_ms == 0 {
        return Err(BullError::message("poll_interval_ms must be positive"));
    }
    if options.idle_timeout_ms == 0 {
        return Err(BullError::message("idle_timeout_ms must be positive"));
    }
    let issues = debug_bridge_config_issues(&DebugBridgeConfig {
        url: format!(
            "ws://{}:{}/bull-debug/stream?token={}",
            host_for_url(&options.bind_host),
            options.port,
            options.token
        ),
        bind_host: options.bind_host.clone(),
        token_required: true,
        token_present: true,
        remote_bind_enabled: false,
        visible_remote_bind_toggle: false,
    });
    if !issues.is_empty() {
        return Err(BullError::message(format!(
            "invalid debug websocket options: {}",
            issues.join(", ")
        )));
    }
    Ok(())
}

fn validate_handshake_request(request: &Request, token: &str) -> Result<(), ErrorResponse> {
    if request.uri().path() != "/bull-debug/stream" {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "expected /bull-debug/stream",
        ));
    }
    if query_token(request.uri().query()) != Some(token) {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "missing or invalid debug token",
        ));
    }
    Ok(())
}

fn query_token(query: Option<&str>) -> Option<&str> {
    query?
        .split('&')
        .find_map(|part| part.strip_prefix("token="))
}

fn error_response(status: StatusCode, message: &str) -> ErrorResponse {
    tungstenite::http::Response::builder()
        .status(status)
        .body(Some(message.to_string()))
        .unwrap_or_else(|_| tungstenite::http::Response::new(Some(message.to_string())))
}

fn debug_ws_url(listener: &TcpListener, options: &DebugWsServerOptions) -> BullResult<String> {
    let address = listener
        .local_addr()
        .map_err(|error| BullError::io(&options.database_path, error))?;
    Ok(format!(
        "ws://{}:{}/bull-debug/stream?token={}",
        host_for_url(&address.ip().to_string()),
        address.port(),
        options.token
    ))
}

fn host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn report(
    bind_url: String,
    session_id: String,
    completion_reason: &str,
    client_count: usize,
    events_sent: usize,
    last_sequence: u64,
    mut issues: Vec<String>,
    handshake_accepted: bool,
    session_found: bool,
) -> DebugWsServeReport {
    let stream_observed = events_sent > 0;
    if handshake_accepted && session_found && !stream_observed {
        issues.push("debug_event_stream_empty".to_string());
    }
    issues.sort();
    issues.dedup();
    let server_valid = handshake_accepted && session_found;
    let next_actions = debug_ws_serve_next_actions(&issues);
    DebugWsServeReport {
        schema: "bull.debug-ws-serve-report.v1".to_string(),
        generated_by: "bull-debug-ws-serve".to_string(),
        bind_url,
        session_id,
        pass: server_valid && stream_observed && issues.is_empty(),
        server_valid,
        handshake_accepted,
        session_found,
        stream_observed,
        completion_reason: completion_reason.to_string(),
        client_count,
        events_sent,
        last_sequence,
        issues,
        next_actions,
    }
}

fn debug_ws_serve_next_actions(issues: &[String]) -> Vec<DebugWsServeNextAction> {
    issues
        .iter()
        .map(|issue| DebugWsServeNextAction {
            scope: debug_ws_serve_issue_scope(issue),
            reason: debug_ws_serve_issue_reason(issue),
            action: debug_ws_serve_issue_action(issue),
        })
        .collect()
}

fn debug_ws_serve_issue_scope(issue: &str) -> String {
    if issue.contains("websocket handshake failed") {
        return "debug_ws_handshake".to_string();
    }
    match issue {
        "debug_session_not_found" => "debug_session".to_string(),
        "debug_event_stream_empty" => "debug_events".to_string(),
        _ => "debug_ws_server".to_string(),
    }
}

fn debug_ws_serve_issue_reason(issue: &str) -> String {
    if issue.contains("websocket handshake failed") {
        return "websocket_handshake_failed".to_string();
    }
    issue.to_string()
}

fn debug_ws_serve_issue_action(issue: &str) -> String {
    if issue.contains("websocket handshake failed") {
        return "Connect with path /bull-debug/stream and the current per-session token."
            .to_string();
    }
    match issue {
        "debug_session_not_found" => {
            "Start a persisted debug session before launching the WebSocket stream.".to_string()
        }
        "debug_event_stream_empty" => {
            "Record app, BLE, parser, or command debug events for this session, or increase the idle timeout before validating the stream.".to_string()
        }
        _ => format!("Resolve debug WebSocket server issue {issue}, then restart the stream."),
    }
}
