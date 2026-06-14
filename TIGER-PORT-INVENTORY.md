# Tiger (`tigercraft4/goose`) → Bull port inventory

A scored, per-phase map of everything tiger built since the shared base, so you can
tick what you want to bring into Bull. Read-only analysis — nothing has been ported.

---

## 🚧 Tier 1 port progress (branch `feat/tier1-gen5-protocol`)

Baseline before port: `cargo test` = **837 tests / 78 suite-results, 0 failed** (green).
Legend: ⬜ pending · 🟡 in-progress · ✅ done.

| # | Sub-task | Files | Status |
|---|----------|-------|--------|
| Setup | Fresh branch + baseline build/test green | — | ✅ |
| **G1** | Route historicalData(47)/historicalIMUDataStream(52) to main while `isHistoricalSyncing` | `BullBLEClient+PeripheralDelegate.swift` | ✅ |
| G1-V | RE/goose sweep on touched file clean; Xcode/hardware build deferred to Tier-1 pause | — | ✅ |
| **G4** | R22 realtime (0x10) decode: constant, `R22Whoop5Hr` variant, `parse_r22_payload`, match arms, tests | `protocol.rs` (+ bridge/capture/export arms), `protocol_tests.rs` | ✅ |
| G4-V | build clean · **840 tests (837+3), 0 failed** · RE/goose grep clean (pre-existing BullAPI "parity" line out of scope) | — | ✅ |
| **G2a** | v18 decode: split `7\|9\|12\|18`→`parse_v18_body`, `V18History` variant, match arms, tests | `protocol.rs` (+ arms), `protocol_tests.rs` | ✅ |
| G2a-fix | **Option A (chosen):** preserve existing k18 feature extraction through `V18History` (HR plan + trusted_frames + skin/resp accept + readiness plans). No G2b pipeline (no store inserts / Swift extractors). | `metric_features.rs`, `metric_readiness.rs` | ✅ |
| G2a-tests | Migrate Bull k18 fixtures/assertions (wider than tiger's 3-file patch — Bull has more k18-coupled tests): relocate fixture HR to v18 offset 22; flip `normal_history`→`v18_history` where k18 frames are asserted | `bridge_tests.rs`, `capture_correlation_tests.rs`, `export_tests.rs`, `local_health_validation_suite_cli_tests.rs`, `metric_features_tests.rs`, `metric_readiness_tests.rs`, `fixtures/synthetic/bull_v5_historical_k18_packet.fixture.json` | ✅ |
| G2a-V | build clean · **842 tests (837+5), 0 failed** · RE/goose grep clean | — | ✅ |

> **G2a blocker (awaiting user decision):** Splitting `18` out of `NormalHistory`
> regresses 14 existing `bridge_tests` (+1 `capture_correlation_tests`). Synthetic
> k18 fixtures already fed the **existing** metric-feature extraction (HR via the
> offset-14 marker, plus resp-rate / skin-temp / trusted-frames) as
> `normal_history`. Once k18→`V18History`, those features return `None`, so the
> score/energy/recovery builders lose their inputs. This is the regression tiger's
> commit `39e9a26` ("fix V18 regressions") repairs — it touches `metric_features.rs`
> (the metrics path the task fenced off for G2a) plus the k18 fixtures. Protocol
> decode itself (`parse_v18_body` + 4 protocol tests) is correct and green. Partial
> G2a left **uncommitted** in the working tree pending the call.
| **G3** | Stale-clock: 300s grid snap when `|captured−device|>86_400s`; EVENT (`packet_kind` contains "event") bypass + 2 bridge tests | `historical_sync.rs`, `bridge_tests.rs` | ✅ |
| G3-V | build clean · **844 tests (837+7), 0 failed** · RE/goose grep clean | — | ✅ |
| Final | Full RE/goose sweep clean across all 16 touched files; **844 tests (837+7), 0 failed**; 4 focused commits on `feat/tier1-gen5-protocol` | — | ✅ |

**Scope guard:** decode + preservation only. No `store.rs` insert wiring, no Swift
extractors, no bridge persistence dispatch beyond minimal classification arms.
BullAPI work untouched. **Option A note:** the v18 split unavoidably re-pointed
existing k18 feature/readiness extraction through `V18History` (HR plan,
`trusted_frames`, skin/resp accept, readiness plans) to avoid regressing data the
device already surfaced — wider than tiger's 3-file patch because Bull has more
k18-coupled tests, but still no new G2b surfacing pipeline.

### Tier 1 commits (branch `feat/tier1-gen5-protocol`, off `main`)

```
7ae30a4  fix(ble): route historical body frames to main during active sync          (G1)
89316b6  feat(protocol): decode WHOOP 5.0 R22 realtime packet (0x10)                 (G4)
de63118  feat(protocol): decode WHOOP 5.0 v18 historical bodies                      (G2a)
99bbbf8  fix(historical-sync): bound stale device clocks with a 300s grid snap       (G3)
```

**⏸ PAUSED for hardware verification.** G1 is Swift-only (builds via Xcode, not
`cargo`) — verify on-device that a Gen5 historical sync now completes and that R22
realtime HR / v18 historical bodies decode against a real WHOOP 5.0 before starting
any Tier 2 work. Not pushed (per AGENTS.md; publishing is the user's call).

## 🚧 Tier 2 port progress — G2b unified V24 + v18 biometric surfacing (branch `feat/tier2-biometric-surfacing`, off `main`)

Tier 1 merged to `main` (`17fe526`). Baseline on branch: `cargo build` green.
Architecture decision: **Rust-side ingest** (reuse Tier 1 decode + cargo test
harness; Swift stays thin), mirroring the existing `step_counter` ingest pattern.
Realtime JSON-over-FFI lag fix is **deferred** to a later perf tier (measure first).
Legend: ⬜ pending · 🟡 in-progress · ✅ done.

| # | Sub-task | Files | Status |
|---|----------|-------|--------|
| Setup | Merge Tier 1 → `main`; branch `feat/tier2-biometric-surfacing`; build green | — | ✅ |
| **T2-1** | `insert_gravity2_batch` + `gravity2_samples_between` (port tiger, RE-scrub) + round-trip/idempotency/inverted-window tests | `store.rs` | ✅ |
| T2-1-V | build clean · **846 tests (844+2), 0 failed** · `store.rs` goose/RE sweep clean | — | ✅ |
| **T2-2** | Generic Rust ingest `run_biometric_ingest_for_store`: read `decoded_frames_between`, match `V24History`+`V18History`, route gravity→`gravity`, gravity2→`gravity2_samples`, skin_temp/spo2/resp→`insert_v24_biometric_batch`; skin-temp plausibility gate, contact gating, idempotency; 4 tests | new `biometric_ingest.rs` | ✅ |
| T2-2-V | build clean · **850 tests (846+4), 0 failed** · new-file goose/RE sweep clean. HR/RR left to `metric_features`; steps left to `step_counter` (single owner per stream) | — | ✅ |
| **T2-3** | Bridge dispatch + registration: `biometrics.ingest_from_decoded` + `biometrics.gravity2_between`; `BRIDGE_METHODS` updated; consistency test + end-to-end wiring test | `bridge.rs` | ✅ |
| T2-3-V | build clean · **851 tests (850+1), 0 failed** · `bridge_methods_constant_matches_dispatcher` green · additions goose/RE clean | — | ✅ |
| **T2-4** | Verified: generic step discovery already classifies v18 `step_motion_counter` as `step_count`/device_counter (key contains "step", path under `$.body_summary.`) — no v18-specific code needed; regression test added | `step_packet_discovery_tests.rs` | ✅ |
| T2-4-V | **852 tests (851+1), 0 failed**. Steps surface for free via existing discovery→ingest→`step_counter_samples`; Swift already calls `metrics.step_counter_ingest` | — | ✅ |
| **T2-5** | Metrics correctness fold-in (tiger Ph 20–35/42): SpO₂/skin-temp/resp scaling, gravity2 → sleep-staging input, recovery Z-weights — only what surfaced numbers depend on | `metric_features.rs`, `sleep_staging.rs`, `metric_readiness.rs` | ⬜ |
| **T2-6** | Swift: `biometrics.ingest_from_decoded` added to `packetInputBridgeReports` (rides existing background pipeline → `packetInputReports`); `localBiometricDeviceID` convention defined. All local SQLite — no HealthKit, no BullAPI | `HealthDataStore+PacketInputs.swift` | ✅ |
| **T2-7** | `DeviceBiometricsView` + model: reads `v24_between` / `gravity_rows_between` / `gravity2_between`, SpO₂ via `spo2_from_raw`, skin-temp raw/128; honest unavailable + uncalibrated states; linked under More → Biometric Engine; pbxproj entries added | `DeviceBiometricsView.swift`, `MoreView.swift`, `project.pbxproj` | ✅ |
| T2-6/7-V | **Simulator build SUCCEEDED** (iPhone 17 Pro); new-file goose/RE sweep clean; only warning is the pre-existing `BullRustBridge` non-Sendable capture pattern shared with `BiometricEnginePreview`. Rust suite unchanged (852, 0 failed) | — | ✅ |
| **V** | After each unit: `cargo build && cargo test --no-fail-fast`; `git grep -i goose` empty; RE sweep clean | — | ⬜ |
| Final | Tracker updated; focused commits; hardware-verify pause | — | ⬜ |

**Scope guard:** stay within Rust core + local SQLite + thin Swift read-back/UI.
**Do not** route physiology through HealthKit (`ios_healthkit_read_boundary_is_weight_only`)
or add a BullAPI write path in this tier — flag for a decision first (as with G2a).

## How to read this

- **Lineage:** `b-nnett/goose` (frozen root) → shared multi-author dev line → two
  sibling forks: **Bull** (this repo; renamed, BullAPI backend, Apple auth, sleep
  hardening) and **Goose** (tiger; kept name, self-hosted FastAPI server, 1061
  commits, v1.0→v10.0).
- **Relevance (1–5)** = value to *Bull specifically* (you are a WHOOP 5.0 user on
  BullAPI; you cannot test Gen4; you do not run tiger's self-hosted server).
  - 5 = fixes a real Gen5 defect or core data correctness
  - 4 = strong correctness/quality win
  - 3 = useful feature or reliability
  - 2 = optional/nice-to-have
  - 1 = not relevant / conflicts with Bull's architecture
- **BullAPI conflict** = targets tiger's `server/` (FastAPI+TimescaleDB). Bull uses
  BullAPI instead, so these need re-pointing, not porting.
- **RE-scrub** = code/comments openly reference Ghidra reverse-engineering of WHOOP
  and parity with proprietary `WHP*` classes. Bull's AGENTS.md forbids RE admissions
  and "we do X because WHOOP does X" framing — must be neutralised before import.
- **Status** = whether Bull already has it.

## ⚠️ Cross-cutting caveat

Tiger's v9.0–v10.0 work is explicitly derived from Ghidra RE of WHOOP v5.37.0
(`.planning/research/whoop-re/WHOOP-GOOSE-CROSS-COMPARE.md`) and names classes for
parity (`WHPBLEBondingManager`, `WHPNetworkMonitor`, `WHPHeartRateDataSanitizer`,
`WHPStateMachine`, `WHPBLEHistoricalDataManager`). Any port must strip the RE
narrative and re-justify on Bull's own principles (data provenance, local-first,
honest empty states). This is a legal-positioning requirement, not a style note.
The mandatory neutralization rules below apply to **every** ported item.

---

## RE-scrub policy (mandatory for every ported item)

Grounded in Bull's AGENTS.md legal positioning. No ported code, comment, commit
message, doc, or test enters Bull until it satisfies all of the following.

1. **Drop all reverse-engineering admissions.** Remove references to Ghidra,
   BTSnoop captures, decompilation, `.planning/research/whoop-re/`,
   `WHOOP-GOOSE-CROSS-COMPARE.md`, and `v5.37.0` provenance.
   - ❌ `// Confirmed via Ghidra decompilation of SetAlarmInfoCommandPacketRev4`
   - ✅ `// Alarm command layout: <field offsets> (parsed from the device's BLE command channel)`

2. **Remove all "parity with WHOOP class X" framing.** Describe what the Bull type
   does, not what WHOOP class it mirrors.
   - ❌ `GooseBLEBondingManager (WHPBLEBondingManager parity)`
   - ✅ `BullBLEBondingManager — formal 5-state bonding lifecycle with bond-loss recovery`

3. **Neutralize capability language.**
   - ❌ "reverse-engineered the v18 historical format / firmware / proprietary protocol"
   - ✅ "parses the v18 historical body exposed by the device over Bluetooth"

4. **Re-ground justifications on Bull's own principles** (data provenance,
   local-first, honest empty/unavailable states). Never "because WHOOP does X",
   "to match WHOOP", "same as the official app", etc.

5. **Keep what AGENTS.md explicitly allows:** compatibility statements
   ("Local Companion for WHOOP 5.0"); functional code symbols already in our tree
   (`OFFICIAL_WHOOP_LABEL_POLICY`, bridge method names, fixtures) — do not rename
   these for cosmetics (it breaks build/tests).

6. **Do not import tiger's `.planning/research/whoop-re/` docs** — pure RE
   narrative, no functional value.

7. **Verify after each port:**
   - `git grep -i goose` → empty
   - `git grep -iE 'ghidra|whp[a-z]|reverse.?eng|decompil|btsnoop|parity|v5\.37'`
     on imported files → clean
   - `cd Rust/core && cargo build && cargo test` → green (74 suites)

---

## Confirmed Gen5 defects in Bull TODAY (verified against our tree)

| ID | Defect | Where | Tiger fix | Relevance |
|----|--------|-------|-----------|-----------|
| G1 | Historical sync never completes — "no packet47 bodies" although band streams them | `BullBLEClient+PeripheralDelegate.shouldDispatchNotificationSideEffectsToMain` drops historicalData/IMU off-main | Phase 67 BLE5-02 + commit `ca6e93f` (route to main while syncing) | **5** |
| G2a | v18 per-second Gen5 biometrics silently discarded (HR/RR/gravity/steps/skin-temp) — **decode** | `protocol.rs` lumps `7\|9\|12\|18` into one HR-marker arm | Phase 67 BLE5-02 (`parse_v18_body` + `V18History` variant) | **5** |
| G2b | decoded v18/v24 fields never reach typed tables / metrics / UI — **surface** | Bull's biometric pipeline is scaffolded but **not wired end-to-end** (see finding below) | tiger v5.0/v6.0 (Phases 20–45), not Phase 67 | **5 (large)** |
| G3 | Stale-clock duplicate/corrupt rows on RTC reset | `historical_sync.rs` timestamp converter | Phase 67 BLE5-02 (300s grid snap + EVENT bypass, `fb4df80`) | **4** |
| G4 | **R22 realtime (0x10) not decoded** — WHOOP 5.0 streams R22 on handle 0x0022; absent in our `protocol.rs` | `protocol.rs` | Phase 67 BLE5-01 (`parse_r22_payload`, `r22_whoop5_hr`) | **4** |
| G5 | Long first-sync can be jetsam-killed (unbounded per-packet writes) | `+HistoricalHandlers` | Phase 68/commit `b0f994f` (batch flush per 32 via `capture.import_frame_batch`) | **3** |

---

## Critical finding: Bull's biometric pipeline is scaffolded, not wired end-to-end

Verified against our tree (2026): the v5.0 biometric pipeline exists only as
storage plumbing. Making decoded Gen5 biometrics actually appear in the app is a
larger project than the Phase 67 protocol fixes, because Bull never wired the
pipeline through — even for V24.

| Layer | State in Bull |
|-------|---------------|
| Tables (`gravity2_samples`, `skin_temp_samples`, `step_counter_samples`, v24) | ✅ exist |
| Store insert methods (`insert_gravity_rows`, `insert_v24_biometric_batch`, `insert_step_counter_sample`) | ✅ exist |
| Bridge **registration** of those inserts | ⚠️ only `store.insert_gravity_rows` registered; v24-batch & step **not dispatched** |
| **Swift caller** that extracts decoded fields → calls inserts | ❌ none (`insertGravity` etc. absent) |
| Extraction extensions (`HealthDataStore+V24Biometrics/+IMUSteps/+Exercise/+Recovery/+Readiness/+StagingSleep`) | ❌ all missing (tiger net-new) |

Consequence: `metrics.sleep_staging` reads `gravity_rows_between`, but **nothing
inserts gravity rows from Swift** — the tables are empty plumbing. So the decoded
bodies have nowhere to surface until the extraction + wiring layer is built.

### G2 therefore splits in two

- **G2a — decode (Tier 1, READY, self-contained):** add `V18History` +
  `parse_v18_body`, split the `7\|9\|12\|18` arm. Gen5 historical frames get
  correctly decoded and preserved in `decoded_frames` with full body JSON instead
  of being silently discarded. Pure `protocol.rs` + tests.
- **G2b — surface (Tier 2, NOT a quick port):** route v18/v24 fields → typed
  tables → metrics → dashboards. Requires bridge registration + the 6
  `HealthDataStore+` extraction extensions + score wiring — effectively tiger's
  **v5.0 + v6.0** (Phases 20–45). Bull lacks this for V24 too.

### Corrected Tier 1 scope (ready now, no further tiger digging needed)

**G1 (routing) + G3 (stale-clock) + G4 (R22) + G2a (v18 decode).** After this:
Gen5 historical sync **completes**, realtime R22 HR is parsed, and rich v18/v24
bodies are captured/stored (no more silent loss) — verifiable on hardware before
committing to the larger Tier 2 pipeline.

---

## Milestone-by-milestone inventory

### v1.0 — Remote Server + Upstream PRs (Phases 1–5)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 1 | FastAPI+TimescaleDB server in `server/`, Dockerised | self-hosted persistence | 1 | **BullAPI conflict** — we have our own backend |
| 2 | iOS server settings (URL/token, Keychain) | configure upload target | 1 | BullAPI conflict |
| 3 | iOS upload client (POST /v1/ingest-decoded, retry) | auto-upload | 1 | BullAPI conflict |
| 4 | Upload status feedback (healthz, last upload, pending) | visibility | 2 | concept reusable for BullAPI UI |
| 5 | Integrate 9 upstream b-nnett PRs | stay current with root | 3 | check which PRs we already have |

### v2.0 — Multi-Device & Platform Foundations (Phases 6–8.1)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 6 | WHOOP 4.0 (Gen4) iOS support (generation field, guards, onboarding) | Gen4 users | 2 | **untestable for you** |
| 7 | Android port foundations (cargo-ndk, JNI shim, ADR) + server CI | future Android | 2 | only if Bull wants Android |
| 8 / 8.1 | Standard HR GATT (`heart_rate_gatt_protocol.rs`, 0x2A37), BLE HR monitor, decoded hr/rr in upload | external HR straps | 3 | net-new Rust file; useful if you want 3rd-party HR |

### v3.0 — Wearable UX, CI Hardening & RTC Sync (Phases 9–15)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 9 | **BLE stability**: FFI catch_unwind, 24MB storage cap, reconnect backoff, device_id propagation | resilience/data integrity | 4 | check overlap with our perf work |
| 10 / 10.1 | HR monitor scan/connect UI; main-thread `@Published` fix (23 methods) | UX + race fixes | 3 | main-thread fixes worth reviewing |
| 11 | HR monitor independent capture | capture without WHOOP | 2 | depends on HR-strap interest |
| 12 | WHOOP 4.0 RTC clock sync | Gen4 drift | 2 | untestable for you |
| 13 | Recovery V2 dashboard (bridge-backed) | real recovery UI | 3 | compare to ours |
| 14 | pt-PT localisation (597 strings) | Portuguese | 1 | not relevant to Bull |
| 15 | **Recovery SDNN accuracy fix** (RMSSD→SDNN, baseline 50ms) | correct recovery formula | 4 | correctness — review vs our recovery_rollup |

### v4.0 — Security, Performance & Coach Expansion (Phases 16–19)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 16 | Deep-link security guard (block state-changing `gooseswift://`) | security | 4 | we have a guard (`Block state-changing debug deep links`) — verify parity |
| 17 | Full `@Observable` migration | perf/correctness | 3 | check our migration state |
| 18 | 4-provider AI Coach (ChatGPT/Claude/Custom/Gemini) + registry + picker | coaching | 2 | feature choice; ~12 files; may overlap our coach |
| 19 | pt-PT for v4.0 strings; non-blocking startup | l10n/UX | 1–2 | startup win maybe useful |

### v5.0 — Metrics Accuracy, IMU & Upstream Fixes (Phases 20–35)  ← **core value**
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 20–35 | **HRV** (BLE-gap-aware RMSSD + Lipponen-Tarvainen filter); **Sleep staging** (Cole-Kripke scale=0.001 + 4-class AASM); **Strain/Calories** (Keytel/Harris-Benedict, Ghidra-confirmed coeffs); **V24 biometric decode** (SpO₂/skin_temp/resp/gravity2); **Exercise detection** (Karvonen zones); **Readiness** (ACWR + Foster monotony); schema v19; 128 Rust tests; 9 HIGH audit fixes | metrics that align with reality | **5** | the heart of "good numbers"; large, touches protocol/metrics/store; some RE-confirmed constants |

### v6.0 — UI Wiring, Algorithm Alignment & Parity Validation (Phases 36–45)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 36–41 | Wire v5.0 algos into dashboards (Readiness, 4-class hypnogram, V24 biometrics, exercise, upload-sync UI, IMU steps) | make v5.0 visible | 4 | depends on taking v5.0 |
| 42 | **Algorithm alignment fixes** (recovery Z-weights HRV .60/RHR .20/resp .05/sleep .15; EWMA α=0.0483 14-night; Cole-Kripke 30s epoch) | correctness vs Python ref | **5** | concrete formula corrections |
| 43–44 | HRV / sleep staging synthetic parity fixtures | validation | 3 | test assets |
| 45 | pt-PT finish; trust-chain import button; Test Connection; `upload.get_raw_frames_for_upload` | l10n + import UX | 2 | import concept reusable |

### v7.0 — Sync Correctness, Async & Sleep Sync (Phases 46–50)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 46 | Server raw-frame ingest/export endpoints | round-trip | 1 | BullAPI conflict |
| 47 | device_uuid end-to-end (CoreBluetooth→SQLite→server) | identity correctness | 3 | Rust/iOS parts useful; server part = BullAPI conflict |
| 48 | **Upload sync race fix** (pre-capture rowIDs, mark synced only after 2xx) | data loss/dupe prevention | 4 | pattern applies to BullAPI upload too |
| 49 | **HealthDataStore full async migration** (60+ calls off main thread) | no main-thread freeze | 4 | big quality win; check our async state |
| 50 | **Morning band sleep sync** (gravity K18/K24 → Cole-Kripke → external_sleep_sessions) | sleep without server | 4 | depends on v5.0 sleep staging |

### v8.0 — Quality, Completeness & Backlog Clearance (Phases 51–59)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 51 | **Bug audit** — 3 HIGH (data race, shared bridge instance, main-thread FFI) + 6 MEDIUM (isFinite guards, RFC3339 parser, pagination guard, fallback constants) | correctness | 4 | review each vs our tree |
| 52 | Quick tasks (BT settings, CodeQL, HealthKit importer, debug-gated previews) | cleanup | 2 | mostly trivial |
| 53 | Home dashboard completion (device status, tools grid, evidence footer) | UI | 2 | feature/UX |
| 54–55 | Coach score summaries, journal, 4 route views | coaching UI | 2 | feature choice |
| 56 | **Remove fabricated 55.0 bpm RHR baseline**; mask exercise HR from stress | honesty/correctness | 4 | aligns with Bull "no guessed values" |
| 57 | Daily energy rollup persistence; real calibration train/holdout | correctness | 3 | review |
| 58 | More tab actions; previews; algorithm-preference shims | UX | 2 | |
| 59 | Band sleep import status string (pipeline done in Ph 50) | UX polish | 2 | |

### v9.0 — BLE Reliability & Protocol Parity (Phases 60–65)  ← **RE-scrub required**
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 60 | Band-first sync (foreground trigger + silent push + BGAppRefreshTask) | sync architecture | 3 | partly BullAPI-coupled |
| 61 | BLE bonding state machine (5-state, CBError 14/15 bond-loss recovery) | reliable bonding | 3 | RE-scrub; net-new file |
| 62 | Per-sensor upload watermarks | incremental upload | 2 | BullAPI conflict + RE-scrub |
| 63 | Network monitor + upload gating + backoff | upload reliability | 2 | BullAPI conflict + RE-scrub |
| 64 | **HR spike sanitizer** (valid 25–220 bpm) | clean HR data | 4 | correctness; RE-scrub; net-new file |
| 65 | Generic BLE state machine type | architecture | 2 | RE-scrub |

### v10.0 — Protocol Parity, Haptics & Feature Completeness (Phases 67–73, active)
| Ph | What | Why | Relevance | Notes |
|----|------|-----|-----------|-------|
| 67 | **WHOOP 5.0 protocol fixes** — R22 realtime (G4), v18 historical decode (G2), stale-clock dedup (G3). Pure Rust. | the Gen5 defects | **5** | **top priority; covers G2/G3/G4** |
| 68 | `GooseBLEHistoricalManager` extraction + `GooseBLEDataValidator` (structural frame checks) | decoupling + bad-frame guard; includes batch-flush (G5) | 3–4 | manager = optional; validator + batch-flush useful |
| 69 | Data foundation — 4 SQLite tables (journal/workout/appleDaily/metricSeries) v19→v20 migration; `GooseStrainAccumulator` actor (live Edwards Zone Load) | live strain + new data | 3 | migration must reconcile with our schema |
| 70 | Haptics (`buzz` cmd 0x13) + Breathe screen | wellness feature | 2 | self-contained feature |
| 71 | Coach VOW nudge; Interval Timer; Metric Explorer; 3 local notifications; HR chart decimation | features + perf | 2–3 | HR decimation (>1000 pts) is a perf win |
| 72 | Stress/ANS view; Trends dashboard (sparklines); Manual workout entry; **Swift protocols + mocks + test target** | UI + testability | 2–3 | ARCH-01 mockability is a real quality win |
| 73 | Smart alarm UI (arm/cancel, buzz confirm); WakeWindow engine **stub** (RE-gated, non-functional) | alarm | 1–2 | engine blocked on more RE; skip |

---

## Recommended pick (for a Gen5 user on BullAPI)

**Tier 1 — do first (fixes your actual problem, ~Rust-only + 1 Swift file):**
- Phase 67 (G2 v18 decode, G3 stale-clock, G4 R22) + the `ca6e93f` routing fix (G1).
  This makes Gen5 historical sync *complete* and the synced data *correct*.

**Tier 2 — correctness/quality (the "good numbers" core):**
- v5.0 metrics (Phases 20–35) + v6.0 Phase 42 alignment fixes.
- v8.0 Phase 51 bug audit + Phase 56 (kill fabricated baselines).
- v3.0 Phase 15 (SDNN fix), v9.0 Phase 64 (HR sanitizer), v7.0 Phase 48/49 (race fix + async).

**Tier 3 — reliability/optional:**
- Phase 68 (validator + batch-flush G5), Phase 9 BLE stability, Phase 71 HR decimation,
  Phase 72 ARCH-01 mocks.

**Skip / re-point to BullAPI, don't port:**
- v1.0 server, v7.0 upload endpoints, v9.0 watermarks/network gating (BullAPI conflict).
- Gen4-specific (Ph 6/12), pt-PT (Ph 14/19/45), wake-window engine (Ph 73 HAP-04).

**Every Swift/v9–v10 item:** rename `Goose*`→`Bull*` and strip RE/WHP-parity narrative.
