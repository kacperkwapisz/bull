# Bull Core Fixtures

Fixtures in this directory must have a sibling `.fixture.json` sidecar.

The sidecar records source, date, device/app version, schema, sensitivity, and
expected parser behavior. Synthetic fixtures contain no user data; captured
fixtures should only be added after sanitization.

Current frame fixtures intentionally stop at stable evidence:

- command frame parity
- event header parsing plus raw body preservation
- K18 historical data-packet header parsing, HR-marker summary, and raw body
  preservation
- R17 optical/filter offset summary with signed sample stats
- shortened K10/K21 motion offset summaries with deliberate truncation warnings

Current sanitized capture fixtures cover:

- a synthetic CoreBluetooth-style notification batch using
  `bull.captured-frame-batch.v1`
- multi-frame parser/import coverage for GET_HELLO plus a shortened K10 raw
  motion packet without per-frame sidecars

Do not promote raw event/history bodies into metric fields until captured
fixtures or trusted references prove units and offsets.

Current algorithm fixtures cover hand-derived inputs for:

- `bull.hrv.v0`
- `bull.sleep.v0`
- `bull.strain.v0`
- `bull.recovery.v0`
- `bull.stress.v0`
