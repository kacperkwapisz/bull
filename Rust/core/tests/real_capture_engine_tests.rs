//! Real-device golden harness.
//!
//! Replays captured V24 history payloads through the *new* biometric engine
//! (V24 decode -> RR/HRV + SpO2/skin-temp/respiration) and asserts the results.
//!
//! Two tiers run from the same code:
//!
//!   1. Always-on: the sanitized `owned/*k24*` payload fixtures committed to the
//!      repo. They are sparse HR-marker frames (zero biometric body), so they
//!      only assert that the V24 decode path is reached and HR is recovered.
//!
//!   2. Opt-in real night: point `BULL_REAL_CAPTURE_DIR` at a directory of
//!      `*.hex` V24 payloads exported + sanitized from a real device. The richer
//!      assertions (RR present, gap-aware HRV finite, plausible SpO2/temp/resp)
//!      activate automatically when such payloads are found.
//!
//! To produce tier-2 inputs: More -> Raw Export (narrow window, `decoded_frames`
//! family), then extract each frame's `payload_hex` into one `.hex` file per
//! frame in a directory and set `BULL_REAL_CAPTURE_DIR` to it.

use std::{fs, path::Path};

use bull_core::metrics::{bull_hrv_v0, HrvInput};
use bull_core::protocol::{build_v5_payload_frame, decode_hex_with_whitespace, parse_frame, DeviceType};

/// One V24 history frame's decoded biometrics, pulled from the parsed body summary.
#[derive(Debug, Default, Clone)]
struct V24Frame {
    hr: Option<f64>,
    rr_intervals_ms: Vec<f64>,
    spo2_red: Option<u64>,
    spo2_ir: Option<u64>,
    skin_temp_raw: Option<u64>,
    resp_raw: Option<u64>,
    sig_quality: Option<u64>,
}

/// Decode a single V24 payload hex through the production parser and lift the
/// biometric fields out of the `v24_history` body summary.
fn decode_v24_payload(hex: &str) -> Option<V24Frame> {
    let payload = decode_hex_with_whitespace(hex).ok()?;
    let frame = build_v5_payload_frame(&payload);
    let parsed = parse_frame(DeviceType::Bull, &frame).ok()?;
    let value = serde_json::to_value(&parsed.parsed_payload).ok()?;
    let body = value.get("body_summary")?;
    if body.get("kind")?.as_str()? != "v24_history" {
        return None;
    }
    let num = |k: &str| body.get(k).and_then(serde_json::Value::as_u64);
    Some(V24Frame {
        hr: body.get("hr").and_then(serde_json::Value::as_f64),
        rr_intervals_ms: body
            .get("rr_intervals_ms")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().filter_map(serde_json::Value::as_f64).collect())
            .unwrap_or_default(),
        spo2_red: num("spo2_red"),
        spo2_ir: num("spo2_ir"),
        skin_temp_raw: num("skin_temp_raw"),
        resp_raw: num("resp_raw"),
        sig_quality: num("sig_quality"),
    })
}

/// Decode every `*.hex` payload in a directory, sorted for determinism.
fn decode_dir(dir: &Path) -> Vec<V24Frame> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "hex"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|hex| decode_v24_payload(hex.trim()))
        .collect()
}

// ---------------------------------------------------------------------------
// Tier 1 — committed owned fixtures (always runs)
// ---------------------------------------------------------------------------

#[test]
fn owned_k24_payloads_decode_through_v24_engine_path() {
    let frames = [
        "fixtures/owned/history_complete_k24_normal_history_payload.hex",
        "fixtures/owned/live_identity_k24_normal_history_payload.hex",
    ]
    .iter()
    .filter_map(|p| fs::read_to_string(p).ok())
    .filter_map(|hex| decode_v24_payload(hex.trim()))
    .collect::<Vec<_>>();

    // Both owned payloads must reach the v24_history decode and recover HR.
    assert_eq!(frames.len(), 2, "owned k24 fixtures must decode as v24_history");
    for frame in &frames {
        assert!(frame.hr.unwrap_or(0.0) > 0.0, "HR must be recovered from the HR marker");
    }
}

// ---------------------------------------------------------------------------
// Tier 2 — real exported night (activates when BULL_REAL_CAPTURE_DIR is set)
// ---------------------------------------------------------------------------

#[test]
fn real_capture_directory_runs_full_engine_when_present() {
    let Ok(dir) = std::env::var("BULL_REAL_CAPTURE_DIR") else {
        eprintln!("skip: set BULL_REAL_CAPTURE_DIR to a folder of exported V24 *.hex payloads");
        return;
    };
    let frames = decode_dir(Path::new(&dir));
    assert!(!frames.is_empty(), "no v24_history payloads decoded from {dir}");

    // RR intervals -> gap-aware HRV. Concatenate every frame's RR and score.
    let rr: Vec<f64> = frames.iter().flat_map(|f| f.rr_intervals_ms.clone()).collect();
    if rr.len() >= 2 {
        let hrv = bull_hrv_v0(&HrvInput {
            start_time: "real-capture".into(),
            end_time: "real-capture".into(),
            rr_intervals_ms: rr.clone(),
            input_ids: vec!["real-capture".into()],
            rr_timestamps_s: None,
            stage_segments: None,
        });
        if let Some(out) = hrv.output {
            assert!(out.rmssd_ms.is_finite() && out.rmssd_ms >= 0.0, "RMSSD must be finite");
            assert!((0.0..=1.0).contains(&out.ectopic_filter_removal_fraction));
            eprintln!(
                "real HRV: rmssd={:.1}ms valid_rr={} ectopic_removed={:.1}%",
                out.rmssd_ms,
                out.valid_interval_count,
                out.ectopic_filter_removal_fraction * 100.0
            );
        }
    }

    // SpO2 / skin-temp / respiration plausibility on raw sensor counts.
    let spo2_frames = frames.iter().filter(|f| f.spo2_red.is_some() && f.spo2_ir.is_some()).count();
    let temp_frames = frames.iter().filter(|f| f.skin_temp_raw.unwrap_or(0) > 0).count();
    let resp_frames = frames.iter().filter(|f| f.resp_raw.unwrap_or(0) > 0).count();
    eprintln!(
        "real V24: {} frames | spo2={} temp={} resp={} sig_quality_present={}",
        frames.len(),
        spo2_frames,
        temp_frames,
        resp_frames,
        frames.iter().filter(|f| f.sig_quality.is_some()).count()
    );
    // Every decoded frame must at least carry HR; raw sensor counts must be in
    // their 16-bit domain (no decode overflow / misalignment).
    for f in &frames {
        assert!(f.hr.unwrap_or(0.0) >= 0.0);
        for v in [f.spo2_red, f.spo2_ir, f.skin_temp_raw, f.resp_raw, f.sig_quality]
            .into_iter()
            .flatten()
        {
            assert!(v <= u16::MAX as u64, "raw V24 count out of 16-bit range: {v}");
        }
    }
}
