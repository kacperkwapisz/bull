# Bull Swift — Performance Measurement Protocol & Baseline

Measurement-only scaffolding is wired in (`BullPerformanceInstrumentation.swift` +
signpost intervals on the suspected hot paths + `_printChanges` on dashboards).
This doc defines **how** we measure and records the **before** numbers so every
later performance PR can post a comparable **after**.

> Rule: no PR may claim a perf fix without a before/after row in the scorecard
> below, captured with the same scenario and build configuration.

---

## Build configurations

- **Release** for FPS / hitch / frame-timing numbers (Debug has no optimizations
  and runs exclusivity checks — it will look artificially slow and mislead).
- **Debug** only for `_printChanges` readability (which dependency re-ran a body).

Device: **iPhone 17 Pro**, iOS 26. Always note OS build in each capture.

---

## Layer 1 — `Self._printChanges()` (which dependency re-rendered a view)

Wired into `HomeDashboardView`, `HealthView`, `CoachView` via
`Self.bullPrintChangesIfEnabled()` (DEBUG-only, gated by a flag).

Enable with **either**:

- Scheme → Run → Arguments → Launch Arguments: `--bull-print-view-changes`
- Or environment variable: `BULL_PRINT_VIEW_CHANGES=1`

Then watch the Xcode console while interacting. Each line names the property that
forced the re-render. **Goal of this layer:** confirm or kill the "fat
ObservableObject over-invalidation" theory. If `HomeDashboardView` re-renders
because of e.g. `historicalPacketCount` or `debugCommandResponses`, the
observation-split work is justified by data, not inference.

Record findings in the table at the bottom.

---

## Layer 2 — `os_signpost` intervals (Instruments timeline)

Subsystem `com.bull.swift`, categories `ui`, `pipeline`, `bridge`. Active
intervals:

| Interval | Category | Location | What it tells us |
|----------|----------|----------|------------------|
| `landingSnapshots` | ui | `HealthDataStore+Snapshots` | Cost of building all landing snapshots on main |
| `healthMonitorSnapshots` | ui | `HealthDataStore+Snapshots` | Cost of vitals/monitor snapshot build |
| `rebuildDisplaySafeMetrics` | ui | `HealthDataStore` | Proves the forbidden-source scan runs once per report, not per body |
| `packetInputReports.assign` | bridge | `HealthDataStore` | Main-thread spike when imports finish |
| `CoachOverviewSnapshot.make` | ui | `CoachView` | Coach rebuild cost (currently per body) |
| `applyPacketUIStateSnapshot` | pipeline | `BullAppModel+PacketPublishing` | Live packet publish work on main actor |

Signposts are near-free unless Instruments records them, so they ship in Release too.

---

## Layer 3 — Instruments templates

Run each scenario (below) with:

| Template | Metric | Provisional target* |
|----------|--------|---------------------|
| **Animation Hitches** (Core Animation) | hitch time ratio (ms/s), min FPS during scroll | 0 hitches; ≥ 55 FPS sustained |
| **Time Profiler** | top 5 main-thread stacks by weight | no single snapshot/`body` symbol dominating |
| **SwiftUI** | `body` evaluations/sec per view | no view evaluating dozens/sec at idle |
| **os_signpost** | durations of the intervals above | correlate spikes with hitches |

\*Targets are provisional until the baseline below is filled; tune after first capture.

---

## Layer 4 — Existing instrumentation to reuse

- `BullRustBridgeTiming` already records encode / FFI / decode microseconds per
  bridge method; surfaced via `recordRustBridgeTiming` → `performance.pipeline`
  OSLog. Use it to confirm whether FFI is a *main-thread* problem or background noise.
- Rust `perf_budget_tests.rs` = **core regression gate only** (desktop CPU), not a
  proxy for iOS frame timing.

---

## Standardized scenarios

Run each 3× and report the **median**. Band connected and streaming unless noted.

1. **Idle Home** — Home tab, live HR updating, no touch (60s).
2. **Scroll Home → Health** — fixed slow drag, alternate tabs (60s).
3. **Sleep / Stress detail** — open chart-heavy detail, scroll (60s).
4. **Active capture/import** — start capture; observe the `packetImportRevision`
   5s-tick window and import-completion spike (60s).

> Determinism caveat: BLE is a live, variable input. For trustworthy deltas, a
> follow-up task should add a **frame-replay mode** feeding recorded notification
> frames through `handleNotification(_:)` at a fixed rate. Until then, keep the
> band in a consistent state and rely on medians.

---

## Reproducible workflow / runbook (how to re-run this analysis)

This is the exact path used for the first baseline. Re-run it verbatim whenever
performance needs reanalyzing (before *and* after a fix), so deltas are comparable.

### 0. Activate the Rust toolchain (required to link the device build)

The app links `libbull_core.a`, built by the "Build Rust Core" Xcode phase. Cargo
lives at `~/.cargo/bin` but may not be on PATH in a fresh shell:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version                       # expect cargo present
rustup target list --installed | rg ios   # expect aarch64-apple-ios (+ -sim)
```

Do **not** set `BULL_SKIP_RUST_CORE_BUILD=1` unless a matching archive already
exists for the active platform — otherwise the link fails with
`library 'libbull_core.a' not found`.

### 1. The three device-id formats (the main footgun)

Each tool wants a **different** identifier for the same phone. Get them once:

```sh
xcrun devicectl list devices     # devicectl/install/launch  -> CoreDevice UUID (9C1B04E2-...)
xcrun xctrace list devices       # xctrace/Instruments        -> hardware ECID  (00008150-...)
# xcodebuild also uses the hardware ECID (00008150-...), NOT the CoreDevice UUID
```

| Tool | ID to use | Example (this Mac/phone) |
|------|-----------|--------------------------|
| `xcodebuild -destination 'platform=iOS,id=...'` | hardware ECID | `00008150-0012221C0240401C` |
| `xctrace record --device ...` | hardware ECID | `00008150-0012221C0240401C` |
| `devicectl device install/launch --device ...` | CoreDevice UUID | `9C1B04E2-19E5-59BF-B17E-9BE055DC7A60` |

### 2. Build + install + launch (device)

```sh
export PATH="$HOME/.cargo/bin:$PATH"
xcodebuild -project BullSwift.xcodeproj -scheme BullSwift -configuration Debug \
  -destination 'platform=iOS,id=<ECID>' \
  -derivedDataPath /tmp/bull-swift-deriveddata-device -allowProvisioningUpdates build

APP=/tmp/bull-swift-deriveddata-device/Build/Products/Debug-iphoneos/BullSwift.app
xcrun devicectl device install app --device <CoreDeviceUUID> "$APP"
xcrun devicectl device process launch --device <CoreDeviceUUID> --terminate-existing com.bull.swift
```

### 3a. Invalidation analysis (the method that worked) — `_printChanges`

Launch with the view-change flag and stream the console to a file while you
interact for ~30 s (scroll Home/Health, open a Sleep/Stress detail):

```sh
( xcrun devicectl device process launch --device <CoreDeviceUUID> \
    --terminate-existing --console \
    --environment-variables '{"BULL_PRINT_VIEW_CHANGES":"1"}' \
    com.bull.swift > /tmp/bull-printchanges.log 2>&1 & )
sleep 32
pkill -f 'devicectl device process launch.*com.bull.swift'   # stop the stream
```

Analyze (these one-liners produced the baseline numbers below):

```sh
wc -l < /tmp/bull-printchanges.log                                   # total body re-renders
rg -o '^(HomeDashboardView|HealthView|CoachView)' /tmp/bull-printchanges.log | sort | uniq -c | sort -rn
rg -c '_model' /tmp/bull-printchanges.log                            # fat-model triggers
rg -c 'CoachView:.*_model'  /tmp/bull-printchanges.log              # off-screen waste
rg -c 'HealthView:.*_model' /tmp/bull-printchanges.log
rg '^HomeDashboardView:' /tmp/bull-printchanges.log | sed 's/HomeDashboardView: //' | sort | uniq -c | sort -rn | head
```

The flag is DEBUG-only (`BullPerfFlags`), so a normal launch stays silent.

### 3b. FPS / hitches / signposts — Instruments

```sh
xcrun xctrace record --device <ECID> --template 'Animation Hitches' \
  --attach BullSwift --time-limit 60s --output /tmp/bull-hitches.trace
```

**Known gotcha:** on iOS 26 `xctrace` may crash on finalize
(`DTKTraceTapMessageHandler` assertion), corrupting the bundle so
`xctrace export` fails with "Document Missing Template Error". Workarounds:
record from the **Instruments GUI** instead (File > Record), or use the
`Time Profiler` / `os_signpost` templates which finalize more reliably. The
signpost intervals (`landingSnapshots`, `applyPacketUIStateSnapshot`,
`rebuildDisplaySafeMetrics`, `packetInputReports.assign`,
`CoachOverviewSnapshot.make`, `healthMonitorSnapshots`) appear on the
**os_signpost** track, subsystem `com.bull.swift`.

---

## Scorecard (append a row per PR)

| Date | Build | Scenario | duration | total body re-renders | Home / Health / Coach | `_model`-driven | off-screen waste | notes |
|------|-------|----------|----------|-----------------------|-----------------------|-----------------|------------------|-------|
| 2026-06-11 | Debug | Mixed interaction (scroll Home/Health + open detail) | ~30 s | **558** | 327 / 120 / 106 | **288** | 189 (Coach 94 + Health 95 re-rendering while off-screen) | **BASELINE**, iPhone 17 Pro, iOS 26.1 |
| 2026-06-11 | Debug | Same scenario, **after Batch 1** | ~30 s | **119** (−79%) | 119 / 0 / 0 | **38** (−87%) | **0** (−100%) | Coach cache + packetImportRevision demote + capture/PacketMonitor/BLE equality guards |

> The `_model`-driven count (288 → 38) is the most trustworthy delta: it is driven
> by the background BLE/capture pipeline, not by how much the screen was scrolled,
> so it is interaction-independent. Off-screen Health/Coach re-renders went to **0**.

### Batch 1 — what shipped (all measurement-safe, no behavior change to data)

1. **Coach overview snapshot caching** (`CoachView`): the 4 `snapshot(for:)` builds +
   summary scans now run only when an input changes (onAppear/onChange), not on
   every `BullAppModel` tick. Off-screen renders use a cheap `.placeholder`.
2. **`packetImportRevision` demoted** from `@Published` to a plain counter
   (`BullAppModel`): no view observed it, but it fired a global re-render every ~5s.
3. **Equality guards** on the high-churn capture publishes
   (`applyHealthPacketCaptureFamilySnapshot`, `publishHealthPacketCaptureUIUpdate`,
   `updateHealthPacketCaptureTargetSummary`), on `PacketMonitorModel.apply`
   (app-wide `@EnvironmentObject`, ~0.2s publish), and on
   `applyBLEUIStateSnapshot` — skip `objectWillChange` when the value is unchanged.

### Methodology correction (important)

The interaction runs (558 → 119 → 236) are **confounded** — each had a different
amount of manual scrolling/tab-switching, which I cannot replicate by hand. Do
**not** compare those totals across builds. The trustworthy, controlled metric is
**idle Home (no touch), band streaming**, which isolates background pipeline churn
(the thing we are optimizing). Until a frame-replay harness exists, use idle.

### Batch 2 — `@self` cascade

- `AppShellView` no longer observes `BullAppModel`. It previously re-ran on every
  model tick and recreated the tab-content structs (new closure identity →
  `@self changed` cascade through the Home tree). Tab selection is still logged
  via each tab's `page.opened` onAppear.

### Controlled idle measurement (the number that matters)

| Build | Scenario | Idle re-renders / 32s | Notes |
|-------|----------|-----------------------|-------|
| Batch 1 + 2 | Idle on Home, no touch | **5** (~0.16/s) | Background churn effectively eliminated; off-screen tabs 0 |

Interpretation: when nothing is happening, the app is silent. Combined with
off-screen tabs at 0 and Coach no longer rebuilding snapshots per tick, the
invalidation problem identified in the baseline is resolved for the idle/steady
state. Remaining interaction-time cost (scroll/navigation) is normal SwiftUI
struct recreation; revisit only if Instruments shows hitches during scroll.

### Still open (optional, reviewed step)

Larger structural move — extract the ~40 capture/overnight/debug `@Published` off
`BullAppModel` into a child `ObservableObject` only `MoreDebugViews` observes
(~130 refs across 7 files; touches the core capture pipeline). Lower priority now
that idle churn is gone; do with a WHOOP band attached to verify capture.

> Note: baseline captured in **Debug** via `_printChanges` (counts re-renders, not
> FPS). Re-capture FPS/hitches in **Release** via Instruments GUI when the
> `xctrace` finalize crash is avoided. Re-render counts are still a valid,
> comparable proxy for the invalidation problem.

### `_printChanges` findings (Layer 1) — baseline 2026-06-11

| View | Dominant trigger | Expected? | Finding |
|------|------------------|-----------|---------|
| HomeDashboardView | `_model` (76×) + `@self` (188×) | No | Visible tab; `@self` = struct recreated by `AppShellView` each model tick, cascading |
| HealthView | `_model` (95×) | **No** | Re-rendering **while off-screen** — pure waste |
| CoachView | `_model` (94×) | **No** | Re-rendering **while off-screen**; each runs `CoachOverviewSnapshot.make` (4 snapshot builds) |

**Raw log:** `/tmp/bull-printchanges.log` (not committed; regenerate via runbook).

---

## Decision gate — RESULT (2026-06-11)

**Invalidation is the #1 cause, confirmed by data, not inference.**

- `_model` (the ~47-`@Published` `BullAppModel`) drove **288 / 558** re-renders.
- **~189 re-renders (~34%) were off-screen tabs** (`TabView` keeps all tabs
  alive; Coach/Health redraw on every model tick while invisible).
- → Proceed with plan Phase 1–2 (observation split). **Highest-ROI first move:**
  stop non-selected tabs from subscribing to live `BullAppModel` ticks and/or
  split live-vitals fields out of `BullAppModel` into a small observable the
  dashboards read. Expected to remove the ~34% off-screen waste immediately.
- Re-run the **§3a runbook** after each fix and append a scorecard row.
