# Algorithm Port Plan — Noop → Bull Rust Core

Reference source: `NoopApp/noop` (Swift, `Packages/StrandAnalytics/`)
Target: `bull-core` (Rust, `Rust/core/src/`)

## Priority order

### 1. Personal Baselines (`baselines.rs`) — HIGH IMPACT
**Source**: `noop-Baselines.swift`
**What**: Winsorized EWMA rolling baselines per nightly metric (HRV, RHR, resp, skin temp).
- EWMA center with configurable half-life (14 nights default)
- EWMA-of-absolute-deviation spread tracker
- Cold-start gating: calibrating (< 4 nights) → provisional (< 14) → trusted
- Hard outlier rejection (> 5× spread), Winsor clamping (± 3× spread)
- Anti-anchoring: faster adaptation during early life (first 8 nights)
- Staleness tracking (> 14 nights without update → stale)
- Z-score deviation: `(value − baseline) / (1.253 × spread)`

**Bull currently**: No personal baselines. Recovery uses fixed thresholds.
**Impact**: Without baselines, recovery/strain scores are meaningless population averages.

---

### 2. Recovery Scorer (`metrics.rs` or new `recovery.rs`) — HIGH IMPACT
**Source**: `noop-RecoveryScorer.swift`
**What**: Z-score + logistic composite → 0–100 recovery.
- Weights: HRV 0.55, RHR 0.20, sleep quality 0.15, resp 0.05, skin temp 0.05
- Each metric z-scored against personal baseline
- Missing terms dropped, weights renormalized
- Composite z → logistic: `100 / (1 + exp(-1.6 × (z − (−0.20))))`
  - Z=0 → ~58% (population average)
  - ±2 z-units ≈ full Red–Green band (15%–95%)
- Cold-start: nil when HRV baseline not yet usable
- Resting HR: lowest 5-min rolling-mean HR during in-bed window
- Bands: Red < 34%, Yellow < 67%, Green ≥ 67%

**Bull currently**: Fixed-weight component sum with hardcoded zone scaling. Same score every day.
**Impact**: Recovery becomes a real personal readiness indicator.

---

### 3. Strain Scorer (`metrics.rs`) — MEDIUM IMPACT
**Source**: `noop-StrainScorer.swift`
**What**: Karvonen %HRR → Edwards TRIMP → logarithmic compression → 0–21 (or 0–100).
- HRmax: Tanaka 2001 (`208 − 0.7 × age`) or observed p99.5 from trailing HR
- Heart Rate Reserve: `HRR = HRmax − RHR`
- Per-sample: `%HRR = (HR − RHR) / HRR × 100`, clamped 0–100
- Edwards 5-zone TRIMP: zone weights 1–5 at 50/60/70/80/90 %HRR cut-offs
  - Each sample contributes `zone_weight × sample_duration_minutes`
- Logarithmic compression: `strain = 100 × ln(TRIMP + 1) / ln(7201)`
  - D = 7201 chosen so top zone × 24h = max score
- Alternative: Banister exponential TRIMP (sex-specific coefficients)
- Min 600 HR readings (10 min @ 1 Hz) or 20 readings spanning 10 min

**Bull currently**: Similar zone-based but with fixed max_hr estimation.
**Impact**: More accurate cardiovascular load tracking.

---

### 4. HRV Analyzer (improve existing `ppg.rs` / `metrics.rs`) — MEDIUM IMPACT
**Source**: `noop-HRVAnalyzer.swift`
**What**: Task Force 1996 RMSSD + SDNN with proper cleaning.
- Range filter: drop RR outside [300, 2000] ms
- Malik ectopic rejection: drop beats deviating > 20% from local 5-beat median
- Minimum 20 clean beats required
- Also computes: SDNN, meanNN, pNN50
- Spot reading quality gate: refuse when > 35% of beats rejected

**Bull currently**: Basic RMSSD from PPG-derived RR intervals. No ectopic rejection.
**Impact**: Cleaner HRV → better recovery scores.

---

### 5. Sleep Staging (major rework of `sleep_staging.rs` + `metric_features.rs`) — HIGH IMPACT, HIGH EFFORT
**Source**: `noop-SleepStager.swift` (1600 lines)
**What**: Gravity-based Cole-Kripke + cardiorespiratory 4-class staging.

**Stage 0 — Sleep/wake detection (gravity stillness spine)**:
- Per-sample gravity change (L2 norm of Δgravity)
- Rolling stillness window (15 min): fraction of "still" samples (Δg < 0.01g)
- Still fraction ≥ 70% → "sleep" flag
- Contiguous run building with gap handling (> 20 min gap breaks a run)
- Merge runs < 15 min into neighbours
- Sessions must be > 60 min
- HR confirmation: mean HR ≤ baseline × 1.05
- Daytime false-sleep guard: center in [11am, 8pm] local → stricter bar
- Off-wrist detection via HR gap analysis

**Stage 1 — Per-epoch features (30s epochs)**:
- Epoch grid: 30s bins over the session
- Per-epoch: summed |Δgravity|, moving fraction, mean HR, RMSSD, SDNN
- DoG-HR variability: σ1=120s minus σ2=600s Gaussian-filtered HR
- Respiration rate + RRV from raw resp channel (or RSA from RR intervals)
- Cole-Kripke score: te Lindert 30s weights [106, 54, 58, 76, 230, 74, 67]

**Stage 2 — Percentile-band classifier**:
- Session-relative percentiles (HR p25/p70, RMSSD p70, etc.)
- Wake: sustained motion + activated cardiac
- Deep: still + low HR + regular respiration + high parasympathetic tone
- REM: still + activated cardiac + irregular respiration
- Light: everything else

**Stage 3 — Post-processing**:
- Median smoothing (5-epoch window)
- Physiology reimposition: no REM in first 15 min, deep in first third
- Fragment merge: absorb sub-3-min stage flecks

**Bull currently**: Crude motion-only with HR wake/sleep threshold. No gravity data, no Cole-Kripke, no per-epoch features.
**Prerequisite**: Need to verify R21 IMU frames carry accelerometer/gravity data and flow into decoded_frames.
**Impact**: Real sleep stages instead of "all awake" or "all asleep".

---

### 6. Sleep Debt (`sleep_debt.rs`) — LOW IMPACT
**Source**: `noop-SleepDebt.swift`
**What**: Rolling sleep debt tracking.
- Sleep need (default 8h) minus actual sleep, accumulated over days
- Exponential decay of older debt

---

### 7. Daytime Stress (`stress.rs`) — LOW IMPACT
**Source**: `noop-DaytimeStress.swift`
**What**: Autonomic stress from waking HRV patterns.

---

### 8. Readiness Engine (`readiness.rs`) — MEDIUM IMPACT
**Source**: `noop-ReadinessEngine.swift`
**What**: "Should you push today?" synthesis.
- HRV vs baseline (Plews/Buchheit)
- RHR drift (Lamberts)
- Respiratory rate drift
- Training load balance (acute:chronic workload ratio, Gabbett)
- Training monotony (Foster)
- Headline: Primed / Balanced / Strained / Run down

---

## Execution plan

**Phase 1 (baselines + recovery)**: Port Baselines EWMA → new `baselines.rs`. Port RecoveryScorer z-score+logistic → replace `bull_recovery_v0` in `metrics.rs`. Wire baselines into the pipeline so each night updates them, and recovery reads from them.

**Phase 2 (strain)**: Port StrainScorer TRIMP → replace `bull_strain_v0`. Add Tanaka HRmax. Wire age/sex from user profile.

**Phase 3 (HRV cleanup)**: Add Malik ectopic rejection to the PPG→RR pipeline. Clean up RMSSD calculation.

**Phase 4 (sleep staging)**: Audit R21 IMU data for gravity/accelerometer. Port Cole-Kripke + cardiorespiratory staging. This is the biggest lift.

**Phase 5 (extras)**: Sleep debt, stress, readiness — after the core 4 are solid.
