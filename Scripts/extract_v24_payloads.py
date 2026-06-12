#!/usr/bin/env python3
"""Extract V24 (packet_k == 24) payloads from a Raw Export bundle into one
``.hex`` file per frame, ready for the real-capture golden harness.

Usage:
    python3 Scripts/extract_v24_payloads.py <export-bundle-dir> <out-dir>

Then run the harness against them:
    BULL_REAL_CAPTURE_DIR=<out-dir> cargo test --test real_capture_engine_tests -- --nocapture

The export bundle dir is the unzipped Raw Export (contains data/decoded_frames.jsonl).
"""
import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    bundle = Path(sys.argv[1])
    out = Path(sys.argv[2])
    jsonl = bundle / "data" / "decoded_frames.jsonl"
    if not jsonl.exists():
        # Allow passing the jsonl path directly too.
        jsonl = bundle if bundle.suffix == ".jsonl" else jsonl
    if not jsonl.exists():
        print(f"error: {jsonl} not found (point at the unzipped export bundle dir)")
        return 1

    out.mkdir(parents=True, exist_ok=True)
    written = 0
    for i, line in enumerate(jsonl.read_text().splitlines()):
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if row.get("packet_k") != 24:
            continue
        payload = row.get("payload_hex") or row.get("frame_hex")
        if not payload:
            continue
        (out / f"v24_{i:06d}.hex").write_text(payload.strip())
        written += 1

    print(f"wrote {written} V24 payload(s) to {out}")
    if written == 0:
        print("note: no packet_k==24 frames in this export. The rich SpO2/temp/resp\n"
              "      biometrics may only arrive via a historical sync — trigger one\n"
              "      and export again, or widen the window/include more families.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
