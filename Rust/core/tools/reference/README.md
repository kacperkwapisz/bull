# Reference Adapters

These scripts are optional local benchmark adapters for
`bull-reference-algo-runner`. They must print `bull.external-reference-output.v1`
JSON to stdout and keep outputs `benchmark-only`.

## NeuroKit2 HRV

```bash
python3 tools/reference/neurokit_hrv.py \
  --input fixtures/synthetic/hrv_bull_v0_hand_derived.json \
  --family hrv \
  --provider external.neurokit2.hrv \
  --output-format bull.external-reference-output.v1
```

With NeuroKit2 installed, the adapter converts RR intervals to peaks and runs
time-domain HRV. Without NeuroKit2, it emits a structured
`neurokit2_not_installed` report. `--allow-hand-derived-fallback` exists only so
CI can test the Bull contract without external Python science packages.

## pyHRV Time Domain

```bash
python3 tools/reference/pyhrv_time_domain.py \
  --input fixtures/synthetic/hrv_bull_v0_hand_derived.json \
  --family hrv \
  --provider external.pyhrv.hrv \
  --output-format bull.external-reference-output.v1
```

With pyHRV installed, the adapter runs the time-domain NNI functions for mean
NN, SDNN, RMSSD, NN50, and pNN50. Without pyHRV, it emits a structured
`pyhrv_not_installed` report. `--allow-hand-derived-fallback` is test-only.

## pyActigraphy Sadeh

```bash
python3 tools/reference/pyactigraphy_sadeh.py \
  --input fixtures/synthetic/sleep_actigraphy_counts_sadeh_hand_derived.json \
  --family sleep \
  --provider external.pyactigraphy.sadeh \
  --output-format bull.external-reference-output.v1
```

With pyActigraphy and pandas installed, the adapter runs Sadeh sleep/wake
scoring against one-minute activity-count input. Without those optional
packages, it emits a structured unavailable report. The fallback flag is
test-only and uses the documented Sadeh formula.

## GGIR Sleep Summary

```bash
python3 tools/reference/ggir_sleep_summary.py \
  --input fixtures/synthetic/sleep_ggir_summary_hand_derived.json \
  --family sleep \
  --provider external.ggir.sleep \
  --output-format bull.external-reference-output.v1
```

This adapter ingests exported GGIR part4/part5-style sleep summary rows. It
accepts fields such as `SptDuration`, `SleepDurationInSpt`, `WASO`,
`dur_spt_min`, and `sleep_efficiency_after_onset`, then emits a benchmark-only
sleep summary with explicit units and GGIR provenance. It is a summary-output
wrapper, not a full local GGIR R pipeline.
