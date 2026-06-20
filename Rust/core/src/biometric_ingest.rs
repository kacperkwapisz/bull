//! Biometric ingest — surface decoded V24 + v18 historical bodies into typed tables.
//!
//! The protocol layer decodes the historical bodies the device exposes over
//! Bluetooth into `DataPacketBodySummary::V24History` / `V18History` and stores
//! them as JSON in `decoded_frames`. This module reads those decoded rows back
//! and routes the per-second physiological streams into their typed destination
//! tables so the metrics layer and UI can consume them:
//!
//! - primary gravity triplet  -> `gravity` (the sleep-staging motion input)
//! - secondary gravity triplet -> `gravity2_samples`
//! - SpO2 raw red/IR          -> `spo2_samples`
//! - skin temperature raw     -> `skin_temp_samples`
//! - respiration raw          -> `resp_samples`
//!
//! Heart rate and RR intervals are intentionally NOT handled here — they already
//! surface through the heart-rate feature path (`metric_features`), and the step
//! counter surfaces through the step ingest pipeline (`step_counter`). Keeping a
//! single owner per stream avoids double-counting.
//!
//! Every value is uncalibrated and derived solely from the connected device's
//! own live sensor data; nothing is imported from third-party health stores.
//! Inserts are idempotent on `(device_id, ts)`, so re-ingesting an overlapping
//! window does not duplicate samples. Implausible readings are gated out rather
//! than guessed.

use serde::{Deserialize, Serialize};

use crate::{
    BullResult,
    protocol::{DataPacketBodySummary, ParsedPayload},
    store::{BullStore, V24BiometricBatch},
};

pub const BIOMETRIC_INGEST_REPORT_SCHEMA: &str = "bull.biometric-ingest-report.v1";

/// Inclusive skin-temperature plausibility gate in degrees Celsius. Raw u16 is
/// converted via `raw / 128.0`; readings outside a physiological wrist range are
/// rejected (honest unavailable state) instead of being stored as noise.
const SKIN_TEMP_MIN_DEG_C: f32 = 5.0;
const SKIN_TEMP_MAX_DEG_C: f32 = 45.0;
const SKIN_TEMP_RAW_SCALE: f32 = 128.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BiometricIngestReport {
    pub schema: String,
    pub generated_by: String,
    pub pass: bool,
    pub device_id: String,
    pub start: String,
    pub end: String,
    /// Decoded data-packet rows considered (after CRC + parse filtering).
    pub considered_frame_count: usize,
    /// Rows skipped because a CRC check failed.
    pub crc_rejected_frame_count: usize,
    pub v24_frame_count: usize,
    pub v18_frame_count: usize,
    /// Newly inserted rows (idempotent: a re-run over the same window reports 0).
    pub gravity_inserted: usize,
    pub gravity2_inserted: usize,
    /// V24 biometric streams are persisted via an idempotent `INSERT OR IGNORE`
    /// batch that does not surface per-stream insert deltas, so these report the
    /// number of plausible samples handed to the batch, not net new rows.
    pub spo2_candidates: usize,
    pub skin_temp_candidates: usize,
    pub resp_candidates: usize,
    pub skin_temp_rejected: usize,
    pub issues: Vec<String>,
}

/// Read decoded frames in `[start, end)` and surface their V24/v18 biometric
/// streams into the typed tables for `device_id`. `start`/`end` are RFC3339
/// `captured_at` bounds (matching `decoded_frames_between`).
pub fn run_biometric_ingest_for_store(
    store: &BullStore,
    device_id: &str,
    start: &str,
    end: &str,
) -> BullResult<BiometricIngestReport> {
    let mut considered_frame_count = 0usize;
    let mut crc_rejected_frame_count = 0usize;
    let mut v24_frame_count = 0usize;
    let mut v18_frame_count = 0usize;
    let mut skin_temp_rejected = 0usize;
    let mut issues = Vec::new();

    // Accumulators keyed by destination table.
    let mut gravity: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut gravity2: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut batch = V24BiometricBatch {
        spo2: Vec::new(),
        skin_temp: Vec::new(),
        resp: Vec::new(),
        sig_quality: Vec::new(),
    };

    // Stream the scan so per-second biometric history never materialises at
    // once; the loop is forward-only so behaviour is identical to a slice walk.
    store.for_each_decoded_frame_between(start, end, |row| {
        if !row.header_crc_valid || !row.payload_crc_valid {
            crc_rejected_frame_count += 1;
            return Ok(());
        }

        let parsed: Option<ParsedPayload> = match serde_json::from_str(&row.parsed_payload_json) {
            Ok(parsed) => parsed,
            Err(error) => {
                issues.push(format!("{} parsed_payload_json invalid: {error}", row.frame_id));
                return Ok(());
            }
        };

        let Some(ParsedPayload::DataPacket {
            timestamp_seconds,
            body_summary: Some(body_summary),
            ..
        }) = parsed
        else {
            return Ok(());
        };

        // The device clock (seconds) is the sample time basis the typed tables
        // and downstream windows use. Without it, samples are not time-locatable.
        let Some(ts) = timestamp_seconds.map(|s| s as f64) else {
            return Ok(());
        };

        match body_summary {
            DataPacketBodySummary::V24History {
                skin_contact,
                spo2_red,
                spo2_ir,
                skin_temp_raw,
                resp_raw,
                gravity_x,
                gravity_y,
                gravity_z,
                gravity2_x,
                gravity2_y,
                gravity2_z,
                ..
            } => {
                considered_frame_count += 1;
                v24_frame_count += 1;
                let contact = skin_contact.unwrap_or(0) == 1;

                if let (Some(x), Some(y), Some(z)) = (gravity_x, gravity_y, gravity_z) {
                    gravity.push((ts, x as f64, y as f64, z as f64));
                }
                if let (Some(x), Some(y), Some(z)) = (gravity2_x, gravity2_y, gravity2_z) {
                    gravity2.push((ts, x as f64, y as f64, z as f64));
                }

                // Optical/temperature/respiration streams are only meaningful
                // when the strap is in skin contact; otherwise we store nothing
                // rather than persisting off-wrist noise.
                if contact {
                    if let (Some(red), Some(ir)) = (spo2_red, spo2_ir) {
                        batch.spo2.push((ts, red as i64, ir as i64, 1));
                    }
                    if let Some(raw) = skin_temp_raw {
                        if push_skin_temp(&mut batch, ts, raw) {
                            // accepted
                        } else {
                            skin_temp_rejected += 1;
                        }
                    }
                    if let Some(raw) = resp_raw {
                        batch.resp.push((ts, raw as i64, 1));
                    }
                }
            }
            DataPacketBodySummary::V18History {
                gravity_x,
                gravity_y,
                gravity_z,
                skin_temp_raw,
                ..
            } => {
                considered_frame_count += 1;
                v18_frame_count += 1;

                // v18 has no skin-contact byte — gravity is not contact-gated.
                if let (Some(x), Some(y), Some(z)) = (gravity_x, gravity_y, gravity_z) {
                    gravity.push((ts, x as f64, y as f64, z as f64));
                }
                if let Some(raw) = skin_temp_raw {
                    if push_skin_temp(&mut batch, ts, raw) {
                        // accepted
                    } else {
                        skin_temp_rejected += 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    })?;

    let gravity_inserted = store.insert_gravity_rows(device_id, &gravity)?;
    let gravity2_inserted = store.insert_gravity2_batch(device_id, &gravity2)?;
    let spo2_candidates = batch.spo2.len();
    let skin_temp_candidates = batch.skin_temp.len();
    let resp_candidates = batch.resp.len();
    store.insert_v24_biometric_batch(device_id, &batch)?;

    Ok(BiometricIngestReport {
        schema: BIOMETRIC_INGEST_REPORT_SCHEMA.to_string(),
        generated_by: "bull-biometric-ingest".to_string(),
        pass: issues.is_empty(),
        device_id: device_id.to_string(),
        start: start.to_string(),
        end: end.to_string(),
        considered_frame_count,
        crc_rejected_frame_count,
        v24_frame_count,
        v18_frame_count,
        gravity_inserted,
        gravity2_inserted,
        spo2_candidates,
        skin_temp_candidates,
        resp_candidates,
        skin_temp_rejected,
        issues,
    })
}

/// Push a skin-temperature sample when the raw reading converts to a
/// physiologically plausible Celsius value. Returns `true` when accepted.
fn push_skin_temp(batch: &mut V24BiometricBatch, ts: f64, raw: u16) -> bool {
    let deg_c = raw as f32 / SKIN_TEMP_RAW_SCALE;
    if (SKIN_TEMP_MIN_DEG_C..=SKIN_TEMP_MAX_DEG_C).contains(&deg_c) {
        batch.skin_temp.push((ts, raw as i64, 1));
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DeviceType, build_v5_payload_frame, parse_frame};
    use crate::store::{DecodedFrameInput, RawEvidenceInput};

    const START: &str = "2026-05-28T00:00:00Z";
    const END: &str = "2026-05-29T00:00:00Z";
    const DEVICE_ID: &str = "bull.test.biometric";

    fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
        buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Build a v24 historical body with all biometric streams populated. `ts`
    /// becomes the device timestamp; `contact` toggles the skin-contact byte.
    fn v24_payload(ts: u32, contact: bool, skin_temp_raw: u16) -> Vec<u8> {
        // payload[0]=HISTORICAL_DATA, [1]=24 (version), [2]=1 (stream).
        // Body parser operates on payload[3..]; timestamp at payload[7].
        let mut payload = vec![0u8; 90];
        payload[0] = crate::protocol::PACKET_TYPE_HISTORICAL_DATA;
        payload[1] = 24;
        payload[2] = 1;
        put_u32(&mut payload, 7, ts);
        // gravity_x/y/z at data[33/37/41] = payload[36/40/44]
        put_f32(&mut payload, 36, 0.1);
        put_f32(&mut payload, 40, 0.2);
        put_f32(&mut payload, 44, 0.98);
        // skin_contact at data[48] = payload[51]
        payload[51] = if contact { 1 } else { 0 };
        // gravity2 at data[49/53/57] = payload[52/56/60]
        put_f32(&mut payload, 52, 0.3);
        put_f32(&mut payload, 56, 0.4);
        put_f32(&mut payload, 60, 0.95);
        // spo2_red data[61]=payload[64], spo2_ir data[63]=payload[66]
        put_u16(&mut payload, 64, 12_000);
        put_u16(&mut payload, 66, 10_000);
        // skin_temp_raw data[65]=payload[68]
        put_u16(&mut payload, 68, skin_temp_raw);
        // resp_raw data[73]=payload[76]
        put_u16(&mut payload, 76, 5_000);
        payload
    }

    /// Build a v18 historical body with gravity + skin temp populated.
    fn v18_payload(ts: u32, skin_temp_raw: u16) -> Vec<u8> {
        let mut payload = vec![0u8; 90];
        payload[0] = crate::protocol::PACKET_TYPE_HISTORICAL_DATA;
        payload[1] = 18;
        payload[2] = 1;
        put_u32(&mut payload, 7, ts);
        // HR at data[22]=payload[25]
        payload[25] = 70;
        // gravity at data[45/49/53]=payload[48/52/56]
        put_f32(&mut payload, 48, 0.05);
        put_f32(&mut payload, 52, -0.05);
        put_f32(&mut payload, 56, 0.99);
        // skin_temp_raw at data[73]=payload[76]
        put_u16(&mut payload, 76, skin_temp_raw);
        payload
    }

    fn seed_frame(store: &BullStore, frame_id: &str, payload: &[u8]) {
        let frame = build_v5_payload_frame(payload);
        let parsed = parse_frame(DeviceType::Bull, &frame).unwrap();
        assert!(
            parsed.header_crc_valid && parsed.payload_crc_valid,
            "test frame must have valid CRCs"
        );
        let evidence_id = format!("{frame_id}.ev");
        store
            .insert_raw_evidence(RawEvidenceInput {
                evidence_id: &evidence_id,
                source: "synthetic.test",
                captured_at: "2026-05-28T01:00:00Z",
                device_model: "WHOOP 5.0 Bull",
                payload: &frame,
                sensitivity: "synthetic",
                capture_session_id: None,
            })
            .unwrap();
        store
            .insert_decoded_frame(DecodedFrameInput {
                frame_id,
                evidence_id: &evidence_id,
                parsed: &parsed,
                parser_version: "biometric-ingest-test",
            })
            .unwrap();
    }

    #[test]
    fn ingests_v24_and_v18_into_shared_typed_tables() {
        let store = BullStore::open_in_memory().unwrap();
        // 32.0 degC raw = 32 * 128 = 4096 (within gate).
        seed_frame(&store, "f.v24", &v24_payload(1_000, true, 4096));
        seed_frame(&store, "f.v18", &v18_payload(1_001, 4096));

        let report = run_biometric_ingest_for_store(&store, DEVICE_ID, START, END).unwrap();

        assert!(report.pass, "issues: {:?}", report.issues);
        assert_eq!(report.v24_frame_count, 1);
        assert_eq!(report.v18_frame_count, 1);
        // Primary gravity: one row from each body.
        assert_eq!(report.gravity_inserted, 2);
        // Secondary gravity: v24 only.
        assert_eq!(report.gravity2_inserted, 1);
        // Contact-gated V24 streams.
        assert_eq!(report.spo2_candidates, 1);
        assert_eq!(report.resp_candidates, 1);
        // skin temp from both bodies.
        assert_eq!(report.skin_temp_candidates, 2);
        assert_eq!(report.skin_temp_rejected, 0);

        // Verify rows actually landed in the typed tables.
        let gravity = store.gravity_rows_between(DEVICE_ID, 0.0, 10_000.0).unwrap();
        assert_eq!(gravity.len(), 2);
        let gravity2 = store.gravity2_samples_between(DEVICE_ID, 0.0, 10_000.0).unwrap();
        assert_eq!(gravity2.len(), 1);
        let v24 = store.v24_biometric_samples_between(DEVICE_ID, 0.0, 10_000.0).unwrap();
        assert_eq!(v24.spo2.len(), 1);
        assert_eq!(v24.skin_temp.len(), 2);
        assert_eq!(v24.resp.len(), 1);
    }

    #[test]
    fn ingest_is_idempotent_on_rerun() {
        let store = BullStore::open_in_memory().unwrap();
        seed_frame(&store, "f.v24", &v24_payload(2_000, true, 4096));

        let first = run_biometric_ingest_for_store(&store, DEVICE_ID, START, END).unwrap();
        assert_eq!(first.gravity_inserted, 1);
        assert_eq!(first.gravity2_inserted, 1);

        let second = run_biometric_ingest_for_store(&store, DEVICE_ID, START, END).unwrap();
        // Re-running the same window inserts no new gravity rows.
        assert_eq!(second.gravity_inserted, 0);
        assert_eq!(second.gravity2_inserted, 0);

        let gravity = store.gravity_rows_between(DEVICE_ID, 0.0, 10_000.0).unwrap();
        assert_eq!(gravity.len(), 1);
    }

    #[test]
    fn off_contact_v24_suppresses_optical_streams_but_keeps_gravity() {
        let store = BullStore::open_in_memory().unwrap();
        seed_frame(&store, "f.v24", &v24_payload(3_000, false, 4096));

        let report = run_biometric_ingest_for_store(&store, DEVICE_ID, START, END).unwrap();

        // Off-wrist: no optical/temp/resp, but gravity is not contact-gated.
        assert_eq!(report.spo2_candidates, 0);
        assert_eq!(report.skin_temp_candidates, 0);
        assert_eq!(report.resp_candidates, 0);
        assert_eq!(report.gravity_inserted, 1);
        assert_eq!(report.gravity2_inserted, 1);
    }

    #[test]
    fn stream_summary_reports_counts_and_latest_without_paging() {
        let store = BullStore::open_in_memory().unwrap();
        seed_frame(&store, "f.v24a", &v24_payload(1_000, true, 4096));
        seed_frame(&store, "f.v24b", &v24_payload(1_002, true, 4224)); // 33.0 degC, latest
        seed_frame(&store, "f.v18", &v18_payload(1_001, 4096));
        run_biometric_ingest_for_store(&store, DEVICE_ID, START, END).unwrap();

        let summary = store.biometric_stream_summary(DEVICE_ID).unwrap();
        assert_eq!(summary.spo2_count, 2);
        assert_eq!(summary.skin_temp_count, 3); // 2 v24 + 1 v18
        assert_eq!(summary.resp_count, 2);
        assert_eq!(summary.gravity_count, 3); // 2 v24 + 1 v18
        assert_eq!(summary.gravity2_count, 2); // v24 only
        // Latest skin temp is from the most recent ts (1_002 -> raw 4224).
        assert_eq!(summary.latest_skin_temp_raw, Some(4224));
        assert!(summary.latest_spo2_red.is_some());
        assert!(summary.latest_spo2_ir.is_some());
    }

    #[test]
    fn stream_summary_is_empty_for_unknown_device() {
        let store = BullStore::open_in_memory().unwrap();
        let summary = store.biometric_stream_summary("bull.test.absent").unwrap();
        assert_eq!(summary.gravity_count, 0);
        assert_eq!(summary.spo2_count, 0);
        assert_eq!(summary.latest_skin_temp_raw, None);
    }

    #[test]
    fn implausible_skin_temp_is_rejected() {
        let store = BullStore::open_in_memory().unwrap();
        // raw 256 -> 2.0 degC, below the 5 degC gate.
        seed_frame(&store, "f.v18", &v18_payload(4_000, 256));

        let report = run_biometric_ingest_for_store(&store, DEVICE_ID, START, END).unwrap();
        assert_eq!(report.skin_temp_candidates, 0);
        assert_eq!(report.skin_temp_rejected, 1);
        // gravity still surfaces.
        assert_eq!(report.gravity_inserted, 1);
    }
}
