# Bull Rust Core

This is the first runnable Bull core scaffold. It starts with the evidence and
validation layer required before UI or BLE controls can be trusted.

## Implemented Crate Areas

- `protocol`: WHOOP v5 frame parsing, CRC validation, deframing, command/event
  payload classification, stable data-packet header parsing, conservative
  APK-offset body summaries for known history/motion packet families, and a
  small command-frame builder used for parity tests.
- `bridge`: app-facing JSON request/response router plus C ABI string wrapper
  for Swift/Dart FFI integration.
- `fixtures`: fixture metadata indexing plus parser fixture coverage for frame
  hex files and captured-frame batch files.
- `capture_sanitize`: text/JSON capture sanitization that removes secrets and
  pseudonymizes account/device identifiers while preserving protocol evidence,
  with next actions for leak or evidence-omission findings.
- `capture_import`: import indexed frame fixtures and app-captured frame
  batches into SQLite as raw evidence plus decoded frame rows.
- `capture_correlation`: body-summary evidence reports that keep parser-derived
  fields out of trusted metric inputs until owned sanitized captures support
  them.
- `metric_readiness`: algorithm input readiness reports that combine capture
  trust with built-in score input requirements before local scores are promoted.
- `metric_features`: first packet-summary-to-score-input feature extraction,
  starting with normal-history heart-rate markers, raw motion signed-i16
  amplitude features, trusted promotion provenance, and window-level
  aggregation for HR/motion score inputs.
- `commands`: direct-send validation matrix for read, normal state-changing,
  and critical state-changing command families.
- `export`: raw timeframe export to JSONL/CSV/SQLite bundles plus
  manifest/checksum validation.
- `privacy_lint`: recursive privacy scanning for exports/logs, including zipped
  bundles, to catch auth tokens, private WHOOP API replay material, direct
  identifiers, emails, and MAC addresses, with rule-specific next actions for
  failed artifacts.
- `property_tests`: deterministic parser/deframer/algorithm stress checks with
  machine-readable pass/fail reports and next actions for failed invariants.
- `perf_budget`: deterministic parser, deframer, score, and export runtime and
  estimated-memory budget checks for mobile viability, with next actions when
  workloads fail.
- `timeline`: app-facing packet timeline rows derived from decoded frames for
  Capture and Debug views.
- `metrics`: built-in algorithm registry plus `bull.hrv.v0` time-domain HRV
  metrics and initial `bull.sleep.v0`, `bull.strain.v0`,
  `bull.recovery.v0`, and `bull.stress.v0` local scores with component
  output, quality flags, and default primary algorithm preferences.
- `reference`: benchmark-only HRV, sleep, strain, and stress reference output
  generation with explicit provider metadata.
- `algorithm_compare`: Bull-vs-reference comparison reports with explicit
  shared-field deltas and non-comparable field notes.
- `calibration`: user-owned label import/list bridge support, date-split linear
  calibration evaluation with holdout metrics, leakage checks, label provenance
  checks, and SQLite run storage.
- `health_sync`: HealthKit/Health Connect dry-run mapping with permission,
  unit, idempotency, provenance, backfill, stale cleanup, and HRV semantic
  guards.
- `debug_ws`: debug session/event recording plus WebSocket bridge contract
  validation for loopback/token policy, event/command schemas, ordering, and
  command-result correlation.
- `debug_ws_server`: loopback WebSocket serving of persisted debug event
  envelopes with token/path enforcement.
- `ui_coverage`: APK UI inventory coverage audit for navigation destinations,
  layout resources, and source UI class categories.
- `store`: SQLite schema and inserts for raw evidence, decoded frames,
  algorithm definitions/runs, metric tables, calibration label/run tables,
  preferences, command validation records, and persisted debug sessions/events.
- `storage_check`: SQLite schema, migration, row-count, integrity, foreign-key,
  and optional self-test validation for Debug/diagnostic workflows, with next
  actions for failed store gates.

The larger crate boundaries are still the target architecture:

- `bull_protocol`: frame, command, event, and data-packet parsing.
- `bull_store`: SQLite schema, inserts, queries, migrations.
- `bull_metrics`: local metric calculations and provenance records.
- `bull_bridge`: FFI-safe request/response structs for Flutter.
- `bull_debug`: event envelope and debug command model shared with the app.

## Tools

Run from this directory:

```sh
cargo test
cargo run --bin bull-fixture-index -- --fixtures fixtures --output fixtures/index.json
cargo run --bin bull-capture-sanitize -- --input ../../captures/sessions/example --output /tmp/bull-sanitized-capture
cargo run --bin bull-parser-fixture-runner -- --fixtures fixtures --index fixtures/index.json
cargo run --bin bull-capture-import -- --fixtures fixtures --index fixtures/index.json --db /tmp/bull.sqlite
cargo run --bin bull-capture-correlation -- --fixtures fixtures --index fixtures/index.json
cargo run --bin bull-command-validator -- --template
cargo run --bin bull-command-validator -- --evidence ../fixtures/command-evidence/whoop-emulator-command-evidence.json --capture-plan --commands toggle_realtime_hr,start_firmware_load_new
cargo run --bin bull-command-validator -- --emulator-log ../../../captures/ble/whoop-emulator.log --emulator-evidence-output ../../../captures/ble/bull-command-evidence.json --visible-user-intent --triggering-ui-action "Official app screen/button used during capture"
cargo run --bin bull-command-validator -- --emulator-log ../../../captures/ble/emulator-command-capture.jsonl --emulator-mirror-local-frame --visible-user-intent --commands get_hello --capture-plan
cargo run --bin bull-reference-algo-runner -- --family hrv --input fixtures/synthetic/hrv_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-hrv-demo-run
cargo run --bin bull-reference-algo-runner -- --family sleep --input fixtures/synthetic/sleep_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-sleep-demo-run
cargo run --bin bull-reference-algo-runner -- --family strain --input fixtures/synthetic/strain_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-strain-demo-run
cargo run --bin bull-reference-algo-runner -- --family stress --input fixtures/synthetic/stress_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-stress-demo-run
cargo run --bin bull-algo-benchmark -- --input fixtures/synthetic/hrv_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id hrv-demo-run
cargo run --bin bull-algo-benchmark -- --algorithm bull.sleep.v0 --input fixtures/synthetic/sleep_bull_v0_hand_derived.json --label-value 86.0 --label-unit score_0_to_100 --label-source manual --label-provenance-json '{"entry":"typed_by_user","official_labels_are_labels":true}' --max-absolute-error 2.0
cargo run --bin bull-algo-benchmark -- --compare-reference --family hrv --input fixtures/synthetic/hrv_bull_v0_hand_derived.json
cargo run --bin bull-algo-benchmark -- --compare-reference --family sleep --input fixtures/synthetic/sleep_bull_v0_hand_derived.json
cargo run --bin bull-algo-benchmark -- --compare-reference --family sleep --input fixtures/synthetic/sleep_bull_v0_hand_derived.json --reference-report /tmp/bull-external-sleep-reference.json
cargo run --bin bull-algo-benchmark -- --compare-reference --family strain --input fixtures/synthetic/strain_bull_v0_hand_derived.json
cargo run --bin bull-algo-benchmark -- --compare-reference --family stress --input fixtures/synthetic/stress_bull_v0_hand_derived.json
cargo run --bin bull-algo-benchmark -- --algorithm bull.recovery.v0 --input fixtures/synthetic/recovery_bull_v0_hand_derived.json
cargo run --bin bull-calibration-evaluator -- --input fixtures/synthetic/recovery_calibration_linear.json --db /tmp/bull.sqlite --run-id calibration-demo-run --split-at 2026-05-04T00:00:00Z
cargo run --bin bull-health-sync-dry-run -- --input fixtures/synthetic/health_sync_dry_run_healthkit.json
cargo run --bin bull-storage-check -- --db /tmp/bull.sqlite --self-test
cargo run --bin bull-debug-ws-contract -- --input fixtures/synthetic/debug_ws_contract_valid.json
cargo run --bin bull-debug-ws-serve -- --db /tmp/bull.sqlite --session-id debug-session-1 --token session-token --port 49152
cargo run --bin bull-ui-coverage-audit -- --input ../apk-ui-inventory/coverage-map.json
cargo run --bin bull-raw-export -- --db /tmp/bull.sqlite --output-dir /tmp/bull.bullbundle --zip-output /tmp/bull.bullbundle.zip --start 2026-05-01T00:00:00Z --end 2026-05-28T00:00:00Z
cargo run --bin bull-export-validator -- --bundle path/to/unpacked-export
cargo run --bin bull-privacy-lint -- --input path/to/unpacked-or-zipped-export
cargo run --bin bull-property-test-suite -- --cases 128 --seed 7453298449734135857
cargo run --bin bull-perf-budget -- --scale 256
```

The import/export smoke path is:

```sh
cargo run --bin bull-fixture-index -- --fixtures fixtures --output fixtures/index.json
cargo run --bin bull-capture-import -- --fixtures fixtures --index fixtures/index.json --db /tmp/bull.sqlite
cargo run --bin bull-reference-algo-runner -- --family hrv --input fixtures/synthetic/hrv_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-hrv-demo-run
cargo run --bin bull-reference-algo-runner -- --family sleep --input fixtures/synthetic/sleep_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-sleep-demo-run
cargo run --bin bull-reference-algo-runner -- --family strain --input fixtures/synthetic/strain_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-strain-demo-run
cargo run --bin bull-reference-algo-runner -- --family stress --input fixtures/synthetic/stress_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id reference-stress-demo-run
cargo run --bin bull-algo-benchmark -- --input fixtures/synthetic/hrv_bull_v0_hand_derived.json --db /tmp/bull.sqlite --run-id hrv-demo-run
cargo run --bin bull-algo-benchmark -- --compare-reference --family hrv --input fixtures/synthetic/hrv_bull_v0_hand_derived.json
cargo run --bin bull-calibration-evaluator -- --input fixtures/synthetic/recovery_calibration_linear.json --db /tmp/bull.sqlite --run-id calibration-demo-run --split-at 2026-05-04T00:00:00Z
cargo run --bin bull-raw-export -- --db /tmp/bull.sqlite --output-dir /tmp/bull.bullbundle --zip-output /tmp/bull.bullbundle.zip --start 2026-05-01T00:00:00Z --end 2026-05-28T00:00:00Z
cargo run --bin bull-export-validator -- --bundle /tmp/bull.bullbundle
cargo run --bin bull-export-validator -- --bundle /tmp/bull.bullbundle.zip
cargo run --bin bull-privacy-lint -- --input /tmp/bull.bullbundle
cargo run --bin bull-privacy-lint -- --input /tmp/bull.bullbundle.zip
```

## App Bridge

The Rust core builds as `rlib`, `staticlib`, and `cdylib`. The first C ABI
surface is declared in `include/bull_core_bridge.h`:

- `bull_core_version_json()`
- `bull_bridge_handle_json(request_json)`
- `bull_bridge_free_string(value)`

Bridge requests use `bull.bridge.request.v1` and responses use
`bull.bridge.response.v1`. Errors are returned as JSON responses, not native
panics.

The Bull Swift app builds this core for iOS through
`../../Scripts/build_ios_rust.sh`. The script selects the Rust target from
Xcode's active platform:

- `PLATFORM_NAME=iphoneos` -> `aarch64-apple-ios`
- `PLATFORM_NAME=iphonesimulator CURRENT_ARCH=arm64` -> `aarch64-apple-ios-sim`
- `PLATFORM_NAME=iphonesimulator CURRENT_ARCH=x86_64` -> `x86_64-apple-ios`

Install the iOS Rust targets before building from Xcode:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

Manual iOS builds from the Swift app repository root:

```sh
PLATFORM_NAME=iphonesimulator CURRENT_ARCH=arm64 Scripts/build_ios_rust.sh
PLATFORM_NAME=iphoneos CURRENT_ARCH=arm64 Scripts/build_ios_rust.sh
```

The generated static libraries are build artifacts staged outside this source
crate at `Rust/iphonesimulator/libbull_core.a` and
`Rust/iphoneos/libbull_core.a`; they should not be committed.

Initial methods:

- `core.version`
- `metrics.built_in_definitions`
- `metrics.reference_definitions`
- `metrics.reference_compare`
- `metrics.bull_hrv_v0`
- `metrics.bull_sleep_v0`
- `metrics.bull_strain_v0`
- `metrics.bull_recovery_v0`
- `metrics.bull_stress_v0`
- `metrics.default_preferences`
- `calibration.import_labels`
- `calibration.list_labels`
- `calibration.apply`
- `export.raw_timeframe`
- `health_sync.dry_run`
- `capture.import_frame_batch`
- `capture.timeline`
- `capture.correlation_report`
- `capture.arrival_plan`
- `metrics.input_readiness`
- `metrics.motion_features`
- `metrics.heart_rate_features`
- `metrics.vital_event_features`
- `metrics.hrv_features`
- `metrics.window_features`
- `metrics.resting_hr_features`
- `metrics.sleep_score_from_features`
- `metrics.recovery_score_from_features`
- `metrics.strain_score_from_features`
- `metrics.stress_score_from_features`
- `diagnostics.perf_budget`
- `commands.evidence_template`
- `commands.validate_evidence`
- `commands.direct_send_gate`
- `commands.capture_plan`
- `commands.list_validation_records`
- `debug.start_session`
- `debug.start_command`
- `debug.finish_command`
- `debug.record_event`
- `debug.session_snapshot`
- `protocol.parse_frame_hex`
- `timeline.from_decoded_frames`
- `storage.check`
- `settings.apply_default_algorithm_preferences`
- `settings.set_algorithm_preference`
- `settings.get_algorithm_preference`
- `settings.list_algorithm_preferences`

This bridge is intentionally batch-oriented. Flutter/Swift should pass JSON
requests and render returned view models; packet schemas and metric formulas
stay in Rust.

Command validation is also bridge-owned. The app should fetch
`commands.evidence_template`, submit official/local comparison evidence through
`commands.validate_evidence` with `persist=true`, and call
`commands.direct_send_gate` before enabling any direct BLE command button. A
future direct send must also pass `commands.direct_send_preflight`, which
requires a ready persisted gate, connected device state, visible user intent,
dry-run bytes shown to the user that match the validated local command frame, a
session log path, and a short-lived override window. Critical commands also
require runtime `critical_visible_confirmation`,
`critical_explicit_approval`, and
`critical_rollback_or_restore_acknowledged` fields, so imported evidence alone
cannot unlock firmware/config/reboot style writes. A missing validation
record fails closed; persisted records keep the
risk gate, direct-send readiness, and full per-command validation result in
SQLite. Persisted command results include the validated evidence source,
capture kind, owner, provenance JSON, and state-changing
`triggering_ui_action` so exported command gates remain auditable outside the
originating database. `commands.capture_plan` sweeps selected command
definitions against those records, embeds each direct-send gate, summarizes
family and critical locks, and emits the next official-capture actions needed
for arrival-day validation. Capture-plan reports expose
`requested_commands_valid`, `validation_records_valid`,
`all_selected_gates_ready`, `critical_gates_ready`, and
`capture_actions_ready`, so a blocked plan identifies the failing proof layer.
`commands.import_validation_records`
can restore exported `data/command_validation.jsonl` rows into a fresh database,
but it is all-or-nothing and refuses ready gates whose embedded result no
longer proves user-owned official-capture provenance or, for state-changing
commands, the official app UI/test action that produced the write. This gate
exists to separate validated controls from guessed or hidden writes;
APK/firmware-derived builders plus official-app-to-emulator captures are
first-class promotion evidence when they preserve bytes, endpoint, write type,
response, UI action, and provenance. The validator does not treat matching hex
strings alone as
enough: the official frame must parse as a Bull
command payload, carry the command number declared for that command definition,
have valid frame CRCs, and include an official response frame that parses as a
command response to the same command before readiness can pass. Command evidence
must also declare a trusted source (`user_owned_official_capture`,
`passive_official_capture`, or a user-owned official-app emulator capture) and
non-empty provenance JSON, so synthetic rows or private API replay material
cannot unlock direct sends. Critical commands additionally require an official
failure response frame that parses back to the same command with a nonzero
result code.
`bull-command-validator --emulator-log` can now parse the macOS spoofed
peripheral log directly into `bull.command-evidence.v1` rows and validate them
without the Python helper chain. It extracts `Write ... command_to_strap` and
`Notify command_from_strap ...` frame pairs, records
`official_app_to_macos_emulator` provenance, and can write the generated import
artifact with `--emulator-evidence-output`. It intentionally leaves
`local_frame_hex` empty unless `--emulator-mirror-local-frame` is passed after a
separate APK/firmware dry-run or safety-gated replay comparison proves the exact
bytes Bull intends to show/send. The same CLI can now take that separate
artifact through `--local-frame-candidates <json-or-jsonl>` and, before
validation, run the exact local-byte promotion step that the Flutter bridge uses.
`--local-frame-match-output <path>` writes the promotion report with matched and
blocked comparisons, so an official-app emulator session plus a Bull dry-run
artifact can be audited end to end without sending a BLE write. The candidate
loader accepts the raw JSON from `whoop-rev build-command GET_HELLO --frame`,
directories of those builder JSON files such as
`015-safe-read-baseline-builders/`, `bull.command-local-frame-candidates.v1`
wrappers, and JSONL rows; CLI style command names such as `GET_HELLO` and
numeric `command_id` values are normalized to Bull command ids before
comparison.
The same parser is available to the Flutter bridge as
`commands.evidence_from_emulator_log`, so the app can import a selected emulator
log and immediately validate/persist the generated gates without shelling out or
performing any BLE write.
Local byte promotion is a separate bridge/tooling step:
`commands.promote_local_frame_matches` takes official evidence plus Bull
dry-run candidates, verifies exact frame bytes, parsed command number, CRCs, and
optional endpoint fields, then emits promoted evidence. Mismatched local bytes
stay blocked as `local_frame_matches_official_frame`.
The emulator evidence report separates official-capture readiness from
local-frame readiness with `official_capture_ready`, `local_frame_match_ready`,
and `direct_validation_ready`. The local-frame match report exposes
`promotion_ready`, `all_frames_matched`, and blocked comparison reasons, and it
fails closed when evidence or candidates are missing.
Command validation reports expose the two halves of that gate explicitly:
`evidence_valid` means the imported evidence rows were known and parseable, while
`all_direct_sends_ready` means every command row in the matrix has passed its
direct-send gate. The top-level `pass` is true only when both are true, so a
partial capture import can still persist useful ready rows but cannot be
misread as a fully unlocked command matrix. When the CLI is run with
`--capture-plan --commands ...`, the process exit instead follows
`evidence_valid && plan.pass` for the selected command scope.

## Fixture Policy

Every fixture file must have a sibling `.fixture.json` sidecar with source,
date, device/app version, schema, consent/sensitivity, checksum coverage, and
expected parser behavior where relevant.

Captured fixtures should only be added after sanitization. Parser code must
preserve unknown bytes and report CRC/shape problems instead of silently
discarding evidence.

`bull-fixture-index` and `bull-parser-fixture-runner` reports include
`next_actions`. Index actions identify missing sidecars, missing metadata,
duplicate ids, unsafe paths, and unreadable content. Parser actions identify
invalid hex, frame parse failures, captured-frame batch JSON/schema/empty-frame
issues, unreadable files, missing expected fields, parsed-payload subset gaps,
and expected-value mismatches so fixture repairs stay evidence-backed.

`bull.captured-frame-batch.v1` fixtures model sanitized CoreBluetooth-style
notification batches. The parser fixture runner validates expected frame count,
packet type names, and parsed payload kinds across the batch, while
`bull-capture-import` flattens each frame into raw evidence and decoded-frame
rows through the same path used by app-captured frame batches.

`bull-capture-sanitize` currently preserves UTF-8 text, JSON, and JSONL
capture evidence and writes a sanitized directory plus `sanitize-manifest.json`.
It redacts token/cookie/authorization fields, pseudonymizes user/account/device
identifiers, redacts emails and bearer/JWT-like tokens in text logs, omits
binary files by default, and fails if sanitized text still contains obvious
credential or identifier markers. Packet/frame hex, timestamps, device model,
firmware/app versions, BLE UUIDs, directions, RSSI, and payload/body hex are
preserved for parser work. Reports include `next_actions` for remaining leak
classes, malformed JSON fallback, and binary omission so the next step is either
to repair sanitizer coverage, convert capture evidence to supported text, or
document a safe omission before parser import.
Reports also expose `input_valid`, `output_ready`,
`supported_files_written`, `unsupported_files_omitted`,
`redaction_scan_clear`, `warnings_clear`, `evidence_complete`, and
`sanitize_ready`; top-level `pass` requires the sanitizer/privacy readiness
fields, while `evidence_complete=false` keeps omitted binaries visible as review
debt.

The app-facing `capture.import_frame_batch` bridge method accepts captured frame
metadata plus frame hex, persists raw evidence first, parses and stores decoded
frames when possible, and returns packet timeline rows for successfully decoded
frames. Malformed but hex-decodable frames remain in `raw_evidence` with parser
issues in the import report. Import reports now include `next_actions` for
empty selections, invalid hex, unreadable fixtures, invalid captured-frame batch
JSON/schema, raw/decoded SQLite insert failures, and parser failures that need a
fixture/regression before the frame can become trusted parser evidence.

The persisted Capture timeline is available through `capture.timeline` with a
database path plus explicit `start` and `end` window. It queries decoded frames
from SQLite and returns the same app-facing packet timeline row model, so the
app does not need to keep import responses in memory to render Capture after a
restart.

Current frame fixtures cover:

- `COMMAND` / `GET_HELLO`
- `EVENT` / event id `17` (`TEMPERATURE_LEVEL`) with raw body preservation
- `HISTORICAL_DATA` / K18 stable header with the HR-present marker at payload
  offset `14`
- `HISTORICAL_DATA` / K17 R17 optical/filter offsets with signed sample stats
- shortened `REALTIME_RAW_DATA` / K10 and `REALTIME_DATA` / K21 motion-summary
  fixtures that prove truncation warnings and raw body preservation
- owned Android btsnoop payload-only fixtures for K24 normal-history, K10 raw
  motion, and K21 grouped motion from live-identity and history-complete
  captures, plus one owned history-complete `TEMPERATURE_LEVEL` event payload
- a synthetic sanitized CoreBluetooth-style batch containing GET_HELLO and K10
  raw motion frames, proving multi-frame parser/import coverage without user
  data

The K18 fixture proves routing, timestamp/header parsing, and marker
preservation. The R17/K10/K21 fixtures prove APK-offset summaries and sample
statistics. They do not claim the physiological body fields are decoded.
Protocol unit tests also cover HR-marker present/absent semantics and
hand-derived body summaries for R17 optical/filter packets, K10 raw motion
packets, and K21 grouped motion packets. These summaries expose offsets,
counts, signed sample previews, and integer stats only; scaling and
physiological units remain capture-gated.

## Export Shape

`bull-raw-export` writes an unpacked `.bullbundle` directory and, when
`--zip-output` is provided, a `.bullbundle.zip` archive with the same manifest
and selected data files. By default it exports every record family and includes
the SQLite family when a database file is available; `--data-families` accepts a
comma-separated subset such as `raw_evidence,decoded_frames,packet_timeline`.

- `manifest.json`
- `data/raw_evidence.jsonl`
- `data/raw_evidence.csv`
- `data/decoded_frames.jsonl`
- `data/decoded_frames.csv`
- `data/packet_timeline.jsonl`
- `data/packet_timeline.csv`
- `data/algorithm_runs.jsonl`
- `data/algorithm_runs.csv`
- `data/calibration_labels.jsonl`
- `data/calibration_labels.csv`
- `data/calibration_runs.jsonl`
- `data/calibration_runs.csv`
- `data/debug_sessions.jsonl`
- `data/debug_sessions.csv`
- `data/debug_commands.jsonl`
- `data/debug_commands.csv`
- `data/debug_events.jsonl`
- `data/debug_events.csv`
- `data/command_validation.jsonl`
- `data/command_validation.csv`
- `data/bull.sqlite` when the `sqlite` family is selected and exporting from a
  file-backed database

Time windows are half-open: `start <= captured_at < end`. The manifest
`data_families` array is the selected family contract; unselected families do
not get files, row counts, or validator requirements.

The same export path is available to the app through `export.raw_timeframe`.
The bridge requires explicit `start` and `end` arguments, accepts
`data_families`, `include_sqlite`, plus optional `zip_output_path`, and returns
the raw export report so Debug/Settings can show row counts, issues, and
generated bundle paths. `bull-export-validator` validates both unpacked
directories and zipped bundles by checking manifest-listed file checksums,
JSONL row counts, selected-family artifact policy, typed raw/decoded/timeline/
command-validation re-importability, raw payload checksums, evidence/frame
references, command-gate provenance, and calibration-label marker/provenance
policy. Reports expose `manifest_valid`, `files_valid`, and `content_valid`;
top-level `pass` requires all three. Failed validation reports include
report-level, file-level, and content-level `next_actions` so Settings, Debug,
and CLI output can point at the concrete regeneration, redaction, checksum,
manifest, or reference fix.

Decoded frame exports include `packet_type_name` and `parsed_payload_json` so
capture review can separate command, event, and data-packet rows while unknown
payload bodies remain available for later parser work. Known K17/K10/K21 packet
families include conservative body summaries with signed sample stats, not
final physiological units.

Packet timeline exports normalize decoded frames into app-facing rows with a
category, title, packet type, sequence, device timestamp where available,
preserved body bytes, summary JSON, and parser warnings. This is the shape the
Capture and Debug screens should render before full physiological parsers are
trusted.

`bull-capture-correlation` emits `bull.capture-correlation-report.v1` for
data-packet body summaries from full frames, payload-only fixtures, captured
frame batches, and SQLite decoded-frame rows. Synthetic fixtures can prove
parser shape, but the report marks summaries as not `trusted_metric_ready`
until the configured minimum owned sanitized capture count is met. Owned
Android btsnoop payload fixtures from two independent captures now satisfy the
default Capture Trust count for K24 normal-history markers, K10 raw motion, and
K21 grouped motion. A history-complete `TEMPERATURE_LEVEL` event payload gives
one owned `event_temperature_level` observation, so it still needs one more
owned event capture and unit/field correlation before score use; R17 optical
still has no owned fixture and needs two owned frames.
Use `--require-owned-captures` in promotion checks before wiring any body
summary into local metric inputs. Blocked summaries include
`next_capture_actions`, so arrival/debug sessions can see exactly how many more
owned frames of each summary kind need live BLE capture or Files import before
rerunning Capture Trust.

`metrics.input_readiness` emits `bull.metric-input-readiness-report.v1` from
the app bridge. It composes the capture correlation report with each built-in
algorithm input contract, counts candidate and trusted evidence by required
summary kind, and keeps score families blocked until the extractor, unit
scaling, and upstream dependency pipeline for every required input is trusted.
Blocked inputs and families include `next_actions` for the next owned capture,
decoder/extractor, or input-mapping task.

`capture.arrival_plan` emits `bull.capture-arrival-plan-report.v1` from the app
bridge. It reruns Capture Trust plus Metric Input Readiness for a local SQLite
window, embeds both reports, dedupes their next actions, and gives the app/debug
stream one checklist of owned captures, extractor work, and score-input blockers
that must clear before trusted local score promotion.

`metrics.motion_features` emits `bull.motion-feature-report.v1` from decoded
raw-motion packets in a local SQLite window. It reads the parser-preserved raw
payload bytes at the axis offsets recorded in the body summary, computes a
preliminary `motion_intensity_0_to_1` from mean absolute signed-i16 amplitude,
and marks each feature trusted only when owned capture correlation has promoted
the underlying motion summary kind.

`metrics.heart_rate_features` emits `bull.heart-rate-feature-report.v1` from
decoded normal-history packets in a local SQLite window. It treats the
parser-preserved nonzero HR marker as a preliminary `heart_rate_bpm` candidate,
keeps marker offset/value provenance, rejects zero and implausible markers, and
marks each feature trusted only when owned capture correlation has promoted the
normal-history summary kind.

`metrics.vital_event_features` emits `bull.vital-event-feature-report.v1` from
decoded vital-like strap events. The first supported candidate is
`TEMPERATURE_LEVEL`; the report preserves the raw event body and owned-capture
trust status, but marks units unresolved and does not promote the value to a
skin-temperature or recovery input until field semantics are proven.

`metrics.hrv_features` emits `bull.hrv-feature-report.v1` from decoded R17
optical/labrador-filtered packets in a local SQLite window. It treats plausible
positive signed-i16 samples in the RR-interval range as preliminary
`rr_intervals_ms` candidates, rejects implausible samples, runs `bull.hrv.v0`
when enough trusted intervals exist, derives daily RMSSD values, can require a
median daily RMSSD baseline, and keeps the scale/provenance flags visible
because the R17 semantics still need official-capture validation.

`metrics.window_features` emits `bull.metric-window-feature-report.v1` by
aggregating trusted HR and motion feature candidates over an explicit local
SQLite window. It reports observed duration, average/max HR, average motion, and
optional HR-zone minutes when a resting/max HR basis is supplied; it does not
invent respiratory, skin-temp, or sleep inputs.

`metrics.resting_hr_features` emits
`bull.resting-heart-rate-feature-report.v1` by reusing trusted heart-rate
candidates. It computes a preliminary current resting HR from the lowest
quartile of local HR markers, derives daily resting-HR values the same way, and
uses the median daily value as a local baseline once enough trusted days exist.
The packet-derived motion, HR, vital-event, HRV, window, and resting-HR feature
reports emit `next_actions` for missing trusted captures, insufficient RR
intervals, baseline-day gaps, and incomplete metric windows before score
composition is attempted.

`metrics.strain_score_from_features` emits
`bull.strain-feature-score-report.v1` by composing trusted resting-HR and
window feature reports into a `bull.strain.v0` input. When no user/profile max
HR is supplied it can use the observed window max as a preliminary max-HR basis
and flags that basis in the score quality flags.

`metrics.sleep_score_from_features` emits
`bull.sleep-feature-score-report.v1` by deriving a preliminary sleep window
from trusted low-motion raw-motion features, then feeding duration, time in bed,
midpoint deviation, and disturbance count into `bull.sleep.v0`.

`metrics.recovery_score_from_features` emits
`bull.recovery-feature-score-report.v1` by composing trusted HRV, HRV baseline,
resting-HR, resting-HR baseline, sleep-score, and prior-strain reports into
`bull.recovery.v0`. Respiratory rate and skin-temperature inputs must be
explicitly provided until packet decoders exist, and the report flags that basis.

`metrics.stress_score_from_features` emits
`bull.stress-feature-score-report.v1` by composing trusted current HR, motion,
resting-HR, current HRV, and HRV-baseline reports into a local
`bull.stress.v0` input/output. It blocks the local score when the current HRV
window or required median daily HRV baseline is missing.

Sleep, recovery, strain, and stress feature-score reports include
`next_actions` when blocked, covering missing trusted motion/HR/HRV/resting-HR
captures, missing HRV/resting baselines, invalid sleep thresholds, missing
provided respiratory/temperature inputs, max-HR issues, metric-window gaps, and
score formula errors that need hand-derived regressions before changes.

`bull-privacy-lint` should pass on both unpacked and zipped exports before a
bundle is shared. It scans UTF-8 files, skips binary SQLite payloads, and fails
on obvious auth headers, unredacted debug `token=` URLs, JWT-like values,
emails, MAC addresses, direct user/device identifiers, and private WHOOP API
replay material. Failed reports include `next_actions` so the app and future
agents can see whether the fix is token redaction, identifier pseudonymization,
or removal of private WHOOP API replay material from shareable artifacts.
The report also exposes `input_valid`, `files_readable`,
`scan_coverage_ready`, `auth_tokens_clear`, `debug_tokens_clear`,
`private_api_clear`, `direct_identifiers_clear`, and `privacy_ready`;
top-level `pass` requires those readiness fields to be true.

Calibration label exports include `official_labels_are_labels=true` on each
row. These rows are comparison/training labels only; they are never presented as
Bull-generated outputs.

## Property Stress Suite

`bull-property-test-suite` emits `bull.property-test-report.v1` JSON and
fails closed when any invariant fails. It uses a deterministic seed, so failures
can be reproduced with the same `--seed` and `--cases` arguments.

The initial suite covers:

- parser invariants for locally built frames, mutated payload CRC failures, and
  arbitrary byte inputs without panics
- deframer invariants for split streams, prefix noise accounting, and exact
  frame byte preservation
- bounds and quality invariants for `bull.hrv.v0`, `bull.sleep.v0`,
  `bull.strain.v0`, `bull.recovery.v0`, and `bull.stress.v0`
- metamorphic checks for sleep duration, strain zone shifts, recovery HRV,
  stress motion adjustment, and constant-RR HRV behavior

Failed group and report rows include `next_actions` scoped to
`group:property:case`, so the next fix names whether to capture failing bytes,
repair parser/deframer no-panic behavior, correct algorithm bounds, or add a
hand-derived monotonic regression.
The report also exposes `input_valid`, `parser_properties_valid`,
`deframer_properties_valid`, `algorithm_bounds_valid`,
`algorithm_metamorphic_valid`, `all_groups_valid`, and
`property_suite_ready`; top-level `pass` requires those readiness fields to be
true.

## Perf Budget

`bull-perf-budget` emits `bull.perf-budget-report.v1` JSON and fails when a
deterministic workload exceeds runtime or estimated working-set budgets. The
initial budget is intentionally conservative for local development: it proves
the Rust path is not obviously hostile to mobile runtime before real iOS device
profiling exists.

The workloads cover:

- parser batch throughput over mixed command/event/data-packet frames
- split-stream deframing with prefix noise and exact frame preservation
- Bull v0 score calculation for HRV, sleep, strain, recovery, and stress
- raw `.bullbundle` plus `.bullbundle.zip` export from a synthetic SQLite
  store

The same report is available to the app through `diagnostics.perf_budget`, and
the Debug tab has a Perf action for local checks. The byte budget is an
estimated retained/output byte budget, not a replacement for later iOS RSS and
energy profiling. Failed report and workload rows include `next_actions` that
distinguish correctness failures, runtime budget misses, memory budget misses,
score-output failures, and raw-export row accounting fixes.
The report also exposes `input_valid`, workload readiness for parser, deframer,
score, and export, plus `duration_budget_ready`, `memory_budget_ready`,
`correctness_ready`, `all_workloads_ready`, and `perf_budget_ready`; top-level
`pass` requires those readiness fields to be true.

## First Algorithms

`bull.hrv.v0` computes time-domain HRV from RR intervals in milliseconds:

- mean NN
- RMSSD
- sample SDNN
- pNN50 using a strict `> 50 ms` threshold

It drops intervals outside `300..=2000 ms`, flags dropped and low-count inputs,
and returns no output when fewer than two valid intervals remain.

The first local score family is also present:

- `bull.sleep.v0`: weighted duration, efficiency, consistency, and
  disturbance components on a 0-100 scale.
- `bull.strain.v0`: HR-zone load plus average HR reserve on a 0-21 scale.
- `bull.recovery.v0`: interpretable HRV, RHR, respiratory, temperature,
  sleep, and prior-strain readiness composite on a 0-100 scale.
- `bull.stress.v0`: HR elevation and HRV suppression with motion-context
  discounting on a 0-100 scale.

Benchmark-only references are registered separately from Bull defaults through
`metrics.reference_definitions` and `bull-reference-algo-runner`:

- `reference.hrv.time_domain.v1`: hand-derived time-domain HRV reference.
- `reference.sleep.actigraphy_summary.v1`: pyActigraphy/GGIR-style sleep window
  summary metrics such as sleep efficiency, WASO, and fragmentation.
- `reference.strain.edwards_zone_load.v1`: Edwards-style weighted HR-zone load.
- `reference.stress.hrv_hr_proxy.v1`: benchmark-only HR/HRV stress proxy.

These are versioned Bull-owned reference reports, not official WHOOP labels.
They are designed to benchmark local formulas while preserving component
breakdowns.

Bull-vs-reference comparisons use `bull.algorithm-comparison-report.v1`.
Current shared fields are HRV time-domain metrics, sleep window and actigraphy
summary fields, strain zone load, and stress HR/HRV proxy components. Score-only
fields that do not have a benchmark equivalent are reported as non-comparable
instead of being forced into misleading deltas.

The core also persists algorithm preferences in `algorithm_preferences`.
Defaults select the built-in Bull v0 algorithm for `hrv`, `sleep`, `strain`,
`recovery`, and `stress` under the `global` scope. Additional scopes, such as a
debug comparison scope, can select different primary algorithms without
overwriting global defaults. The store rejects preferences that point to a
missing algorithm definition or to an algorithm from the wrong metric family.

## Reference Runner

`bull-reference-algo-runner` supports four benchmark-only internal
providers:

- `internal.hand_derived_time_domain`
- `internal.pyactigraphy_style_window_summary`
- `internal.edwards_zone_load`
- `internal.hrv_hr_stress_proxy`

These are benchmark-only internal references, not pyHRV, NeuroKit2,
pyActigraphy, or GGIR executions. They exist to lock down the machine-readable
reference contract before adding named Python/R wrappers.

The runner can also execute an explicit external provider command:

```bash
cargo run --bin bull-reference-algo-runner -- --family hrv --provider external.neurokit2.hrv --input fixtures/synthetic/hrv_bull_v0_hand_derived.json --external-command python3 --external-arg tools/reference/neurokit_hrv.py --db /tmp/bull.sqlite --run-id reference-neurokit-hrv-demo
```

The command receives `--input`, `--family`, `--provider`, and `--output-format`
arguments plus matching `BULL_REFERENCE_*` environment variables. It must emit
`bull.external-reference-output.v1` JSON on stdout with algorithm id/version,
provider version, source, license, parameters, output units, quality gates,
provenance, and errors. Bull validates that contract, records input/stdout
checksums and command metadata, then stores the result as a `benchmark-only`
algorithm definition/run. This adapter is local tooling only; it does not make
external benchmark outputs eligible as primary user-facing scores.

`tools/reference/neurokit_hrv.py`, `tools/reference/pyhrv_time_domain.py`,
`tools/reference/pyactigraphy_sadeh.py`, and
`tools/reference/ggir_sleep_summary.py` are the first named adapters. The
NeuroKit2 adapter wraps `intervals_to_peaks` plus `hrv_time`; the pyHRV adapter
wraps time-domain NNI functions for mean NN, SDNN, RMSSD, NN50, and pNN50; the
pyActigraphy adapter wraps Sadeh sleep/wake scoring for one-minute
activity-count fixtures; the GGIR adapter wraps exported part4/part5-style
sleep summary rows. If optional packages are not installed, dependency-backed
adapters emit structured unavailable reports; tests use
`--allow-hand-derived-fallback` where needed to exercise the contract without
Python science dependencies.

## Algorithm Benchmark

`bull-algo-benchmark` can run a built-in Bull algorithm from JSON input, or
run `--compare-reference` for HRV, sleep, strain, and stress. Comparison mode
emits the
same `bull.algorithm-comparison-report.v1` used by the bridge, including
Bull output, reference output, shared-field deltas, quality flags, runtime,
data coverage, non-comparable fields, and `next_actions` for missing outputs,
algorithm/reference errors, non-finite deltas, invalid reference contracts, or
absent comparable fields. Reports separate `reference_contract_valid`,
`bull_output_ready`, `reference_output_ready`, and `shared_fields_ready` so a
numeric external report cannot pass without provider/provenance and output-unit
metadata.
Sleep comparison can also take `--reference-report` pointing at a
`bull.reference-algo-report.v1` or `bull.external-reference-output.v1` report,
so pyActigraphy/GGIR adapter output can be compared against Bull sleep inputs
without copying external formulas into the mobile runtime.

Built-in algorithm mode emits `bull.algo-benchmark-report.v1` with runtime,
input coverage, the score field selected for comparison, and optional
user-owned label error. Labels are accepted only from allowed calibration
sources, require explicit provenance JSON, and must keep
`official_labels_are_labels=true`. Failed benchmark reports include
`next_actions` for label source/provenance/unit/threshold fixes or algorithm
input requirement failures.

## Calibration Evaluator

`bull-calibration-evaluator` reads `bull.calibration-dataset.v1` records and
fits a one-dimensional linear transform from local predictions to user-owned
labels. Failed evaluator and application reports include `next_actions` for
schema/field fixes, non-finite values, unsupported label sources, missing
provenance, insufficient train/holdout rows, split leakage, zero prediction
variance, holdout non-improvement, mismatched calibration runs, invalid
persisted params, failed runs, and missing models. Reports also separate
`dataset_valid`, `labels_valid`, `split_valid`, `model_fit_ready`,
train/holdout metric readiness, `holdout_improvement_valid`, and
`calibration_ready` so tools do not need to infer why `pass=false`. It requires:

- train rows strictly before `--split-at`
- holdout rows at or after `--split-at`
- no session id overlap across train/holdout
- allowed label sources only
- label provenance on every row
- holdout MAE improvement before the report passes

The synthetic fixture uses recovery-style scores and exists only to prove the
tooling contract. Real labels must come from user-owned manual entry, passive
official-app capture, screenshot import, or user export.

`calibration.apply` applies a stored, passing calibration run to a local Bull
score. It checks the algorithm id/version match, requires a model from a
holdout-passing calibration run, clamps to the score range, and marks the
result as `bull_calibrated_local_score`. Application reports expose
`input_valid`, `score_range_valid`, `calibration_run_valid`, `model_ready`,
`model_applied`, and `application_ready`. Official values remain labels in
provenance; they are not re-emitted as Bull outputs.

## Health Sync Dry Run

`bull-health-sync-dry-run` validates platform mappings before any HealthKit or
Health Connect adapter writes records.

Current checks:

- destination record/type mapping
- exact unit matching
- write permission grant
- half-open backfill window
- user approval
- allowed source kind
- benchmark-only algorithm block
- provenance presence
- idempotency key de-duplication
- Bull marker for later identification
- stale Bull-owned delete planning inside the backfill window
- blocked cleanup reasons for external, unsupported, out-of-window, or
  unpermitted platform records
- HealthKit RMSSD guard: RMSSD is not written as `heartRateVariabilitySDNN`

The dry-run reports blocked records with reasons and `next_actions` for
permission, provenance, unit, idempotency, mapping, RMSSD/SDNN, and cleanup
problems. Top-level readiness fields also separate permission, mapping, unit,
provenance, source-policy, idempotency, and cleanup-scope blockers so app
summaries do not need to infer those causes from row strings. Permission denial
is treated as a safe blocked record, not data loss.

The app can call the same policy through `health_sync.dry_run` before a Swift
HealthKit or Android Health Connect adapter attempts real writes. Dart and the
native HealthKit/Health Connect guards require the report schema/platform,
`pass=true`, `input_valid=true`, and no report-level issues before permission,
write, or cleanup calls can proceed; native adapters still only consume the
Rust-planned write/delete rows.

## Storage Check

`bull-storage-check` validates the local SQLite store before app or Debug-tab
features trust it.

Current checks:

- schema version equals the Rust core version expectation
- `PRAGMA foreign_keys` is enabled on the active connection
- `PRAGMA integrity_check` returns `ok`
- every known Bull table exists
- required columns exist for raw evidence, decoded frames, algorithms,
  calibration, metrics, preferences, command validation, and debug stream state
- row counts are readable for every known table
- optional `--self-test` inserts synthetic raw/decoded evidence, verifies
  idempotency, verifies a query roundtrip, and proves missing-evidence decoded
  frames are rejected by the foreign-key constraint

The default mode does not write self-test rows. Use `--self-test` on a scratch
or intentional diagnostic database. Failed report, table, and self-test rows
include `next_actions` that name whether to run migrations, repair a missing
table/column, stop writes for integrity recovery, re-enable foreign keys, or
fix raw/decoded insert and query roundtrips.
The report also exposes `schema_version_valid`, `foreign_keys_valid`,
`integrity_valid`, `tables_present`, `required_columns_present`,
`row_counts_ready`, `self_test_ready`, and `storage_ready`; top-level `pass`
requires those readiness fields to be true.

## Debug WebSocket Contract

`bull-debug-ws-contract` validates the stream contract that the Debug tab
bridge must satisfy before agents rely on it. The app bridge can persist debug
sessions, command lifecycle events, arbitrary app/core events, and a
contract-validated session snapshot through `debug.*` methods. Contract
reports include explicit readiness bits for input schema, bridge config,
command envelopes, event envelopes, stream ordering, command references,
command result correlation, and overall `contract_ready`.

`bull-debug-ws-serve` is the first local stream transport. It binds to
loopback, requires `/bull-debug/stream?token=...`, streams persisted
`bull.debug.event.v1` envelopes from SQLite in sequence order, and exits after
the configured idle timeout or optional `--max-events`. Its serve report now
separates `server_valid`, `handshake_accepted`, `session_found`, and
`stream_observed`; top-level `pass` requires a valid server/session plus at
least one streamed event. Empty streams, missing sessions, and failed handshakes
emit `next_actions`.

Current checks:

- bridge URL uses a WebSocket scheme and `/bull-debug/stream`
- bridge binds to loopback by default
- per-session token is required and present
- remote bind cannot be enabled without a visible user toggle
- command envelopes use `bull.debug.command.v1`
- event envelopes use `bull.debug.event.v1`
- command args and event data are JSON objects
- event sources, levels, topics, messages, and session ids are valid
- event sequence numbers strictly increase
- event timestamps never move backwards
- every command has `command.started` and `command.result` events
- command events reference known command ids
- contract failures emit `next_actions` for token/path/bind fixes, command
  lifecycle wrapping, event shape repair, sequence/timestamp ordering, and
  unknown command references
- `pass` requires `contract_ready`, and `contract_ready` requires
  `input_valid`, `bridge_valid`, `commands_valid`, `events_valid`,
  `stream_order_valid`, `command_references_valid`, and
  `command_results_correlated`

## UI Coverage Audit

`bull-ui-coverage-audit` validates the generated APK UI inventory against
`projects/bull/apk-ui-inventory/coverage-map.json`.

Current checks:

- navigation destinations resolve to an explicit Bull status
- layout resources resolve through their generated category
- source UI classes resolve through their generated category
- omitted and deferred rules include a reason
- implemented, approximated, and debug-only rules include a target level
- every rule matches at least one current inventory row
- new APK graphs, layout categories, or source categories fail until classified
- reports separate `inventory_valid`, `coverage_map_valid`,
  `all_surfaces_classified`, and `has_deferred_review_debt`, so a passing audit
  can still show named deferred 1:1 coverage debt
- missing, malformed, stale, and deferred surfaces emit `next_actions` that
  name the coverage-map or inventory fix needed before rerunning the audit
