#!/usr/bin/env python3
"""GGIR sleep summary adapter for bull-reference-algo-runner."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

SCHEMA = "bull.external-reference-output.v1"
PROVIDER = "external.ggir.sleep"
ALGORITHM_ID = "reference.sleep.ggir_summary.v1"
ALGORITHM_VERSION = "1.0.0"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Summarize GGIR sleep output for Bull benchmarks."
    )
    parser.add_argument("--input", required=True)
    parser.add_argument("--family", required=True)
    parser.add_argument("--provider", required=True)
    parser.add_argument("--output-format", required=True)
    args = parser.parse_args()

    if args.family != "sleep":
        return write_report(
            base_report({}, "unavailable"),
            errors=[f"unsupported_family:{args.family}"],
        )
    if args.provider != PROVIDER:
        return write_report(
            base_report({}, "unavailable"),
            errors=[f"unsupported_provider:{args.provider}"],
        )
    if args.output_format != SCHEMA:
        return write_report(
            base_report({}, "unavailable"),
            errors=[f"unsupported_output_format:{args.output_format}"],
        )

    try:
        payload = json.loads(Path(args.input).read_text())
    except Exception as exc:  # noqa: BLE001 - external adapters must fail as JSON.
        return write_report(
            base_report({}, "unavailable"),
            errors=[f"input_read_error:{type(exc).__name__}"],
        )

    rows = payload.get("rows", [])
    if not isinstance(rows, list):
        return write_report(
            base_report(payload, provider_version(payload)),
            errors=["rows_must_be_array"],
        )

    summary, quality_flags, invalid_count = summarize_rows(rows)
    if summary is None:
        return write_report(
            base_report(payload, provider_version(payload)),
            quality_flags=quality_flags,
            errors=["no_valid_ggir_sleep_summary_rows"],
        )

    output = summary | {
        "night_count": len(rows),
        "valid_night_count": len(rows) - invalid_count,
        "invalid_night_count": invalid_count,
    }
    return write_report(
        base_report(payload, provider_version(payload)),
        output=output,
        quality_flags=quality_flags,
    )


def base_report(payload: dict[str, Any], provider_version_value: str) -> dict[str, Any]:
    return {
        "schema": SCHEMA,
        "family": "sleep",
        "provider": PROVIDER,
        "provider_version": provider_version_value,
        "source": "GGIR sleep summary export",
        "license": "Apache-2.0",
        "algorithm_id": ALGORITHM_ID,
        "algorithm_version": ALGORITHM_VERSION,
        "display_name": "GGIR Sleep Summary",
        "input_schema": "bull.sleep-ggir-summary-input.v1",
        "output_schema": "bull.sleep-ggir-summary-output.v1",
        "start_time": payload.get("start_time", ""),
        "end_time": payload.get("end_time", ""),
        "output": None,
        "output_units": {
            "night_count": "count",
            "valid_night_count": "count",
            "invalid_night_count": "count",
            "time_in_bed_minutes": "minutes",
            "sleep_minutes": "minutes",
            "wake_minutes": "minutes",
            "sleep_efficiency_fraction": "fraction",
            "wake_after_sleep_onset_minutes": "minutes",
            "disturbance_count": "count",
            "fragmentation_index_per_hour": "events_per_hour",
        },
        "parameters": {
            "accepted_time_in_bed_fields": ["dur_spt_min", "SptDuration"],
            "accepted_sleep_fields": [
                "SleepDurationInSpt",
                "dur_spt_sleep_min",
                "dur_spt_sleep_total_min",
                "sleep_efficiency_after_onset",
            ],
            "accepted_waso_fields": ["WASO"],
            "disturbance_proxy": "max(0, number_sib_sleepperiod - 1)",
            "invalid_row_policy": "drop_and_flag",
        },
        "input_requirements": {
            "rows": {
                "unit": "ggir_part4_or_part5_sleep_summary_rows",
                "minimum_to_compute": 1,
            },
            "time_in_bed": {
                "unit": "minutes",
                "sources": ["dur_spt_min", "SptDuration_hours"],
            },
            "sleep_duration": {
                "unit": "minutes",
                "sources": [
                    "SleepDurationInSpt_hours",
                    "dur_spt_sleep_min",
                    "dur_spt_sleep_total_min",
                    "sleep_efficiency_after_onset",
                ],
            },
        },
        "quality_gates": [
            "external_provider_exit_zero",
            "bull_contract_schema_match",
            "units_recorded",
            "non_empty_provenance",
            "at_least_one_valid_ggir_sleep_summary_row",
        ],
        "quality_flags": [],
        "errors": [],
        "provenance": {
            "adapter": "tools/reference/ggir_sleep_summary.py",
            "input_ids": payload.get("input_ids", []),
            "source_report": payload.get("source_report", ""),
            "library": "GGIR",
            "library_docs": [
                "https://wadpac.github.io/GGIR/articles/GGIRoutput.html",
                "https://www.rdocumentation.org/packages/GGIR/versions/3.2-0/topics/GGIR",
                "https://search.r-project.org/CRAN/refmans/GGIR/html/g.part5.html",
            ],
            "expected_values_policy": "external-summary-contract",
        },
    }


def provider_version(payload: dict[str, Any]) -> str:
    version = str(payload.get("ggir_version", "")).strip()
    return version or "summary-import"


def summarize_rows(rows: list[Any]) -> tuple[dict[str, Any] | None, list[str], int]:
    quality_flags: list[str] = []
    invalid_count = 0
    time_in_bed_total = 0.0
    sleep_total = 0.0
    waso_total = 0.0
    disturbance_total = 0
    valid_count = 0

    for row in rows:
        if not isinstance(row, dict):
            invalid_count += 1
            append_once(quality_flags, "ggir_row_not_object")
            continue

        row_flags = row_quality_flags(row)
        for flag in row_flags:
            append_once(quality_flags, flag)

        spt_minutes = spt_duration_minutes(row)
        sleep_minutes = sleep_duration_minutes(row, spt_minutes)
        if not finite_positive(spt_minutes) or sleep_minutes is None or sleep_minutes < 0.0:
            invalid_count += 1
            append_once(quality_flags, "ggir_row_missing_sleep_fields")
            continue
        if sleep_minutes > spt_minutes:
            invalid_count += 1
            append_once(quality_flags, "ggir_row_sleep_exceeds_spt")
            continue

        waso_minutes = wake_after_sleep_onset_minutes(row, spt_minutes, sleep_minutes)
        if waso_minutes is None or waso_minutes < 0.0:
            waso_minutes = max(0.0, spt_minutes - sleep_minutes)
            append_once(quality_flags, "ggir_row_waso_derived_from_duration")

        time_in_bed_total += spt_minutes
        sleep_total += sleep_minutes
        waso_total += waso_minutes
        disturbance_total += disturbance_count(row)
        valid_count += 1

    if valid_count == 0 or time_in_bed_total <= 0.0:
        return None, quality_flags, invalid_count

    wake_total = max(0.0, time_in_bed_total - sleep_total)
    return (
        {
            "time_in_bed_minutes": time_in_bed_total,
            "sleep_minutes": sleep_total,
            "wake_minutes": wake_total,
            "sleep_efficiency_fraction": sleep_total / time_in_bed_total,
            "wake_after_sleep_onset_minutes": waso_total,
            "disturbance_count": disturbance_total,
            "fragmentation_index_per_hour": (
                disturbance_total / (sleep_total / 60.0) if sleep_total > 0.0 else 0.0
            ),
        },
        quality_flags,
        invalid_count,
    )


def row_quality_flags(row: dict[str, Any]) -> list[str]:
    flags: list[str] = []
    cleaning_code = parse_optional_float(row.get("cleaningcode"))
    if cleaning_code is not None and cleaning_code != 0.0:
        flags.append("ggir_cleaningcode_nonzero")
    acc_available = row.get("acc_available")
    if isinstance(acc_available, bool) and not acc_available:
        flags.append("ggir_accelerometer_unavailable")
    if isinstance(acc_available, str) and acc_available.strip().lower() in {"false", "0", "no"}:
        flags.append("ggir_accelerometer_unavailable")
    return flags


def spt_duration_minutes(row: dict[str, Any]) -> float | None:
    dur_spt = parse_optional_float(row.get("dur_spt_min"))
    if dur_spt is not None:
        return dur_spt
    spt_hours = parse_optional_float(row.get("SptDuration"))
    if spt_hours is not None:
        return spt_hours * 60.0
    return None


def sleep_duration_minutes(row: dict[str, Any], spt_minutes: float | None) -> float | None:
    sleep_hours = parse_optional_float(row.get("SleepDurationInSpt"))
    if sleep_hours is not None:
        return sleep_hours * 60.0
    for field in ("dur_spt_sleep_min", "dur_spt_sleep_total_min"):
        value = parse_optional_float(row.get(field))
        if value is not None:
            return value
    efficiency = parse_optional_float(row.get("sleep_efficiency_after_onset"))
    if efficiency is not None and spt_minutes is not None:
        fraction = efficiency / 100.0 if efficiency > 1.0 else efficiency
        return spt_minutes * fraction
    return None


def wake_after_sleep_onset_minutes(
    row: dict[str, Any],
    spt_minutes: float,
    sleep_minutes: float,
) -> float | None:
    waso_hours = parse_optional_float(row.get("WASO"))
    if waso_hours is not None:
        return waso_hours * 60.0
    return max(0.0, spt_minutes - sleep_minutes)


def disturbance_count(row: dict[str, Any]) -> int:
    value = parse_optional_float(row.get("number_sib_sleepperiod"))
    if value is None:
        return 0
    return max(0, int(round(value)) - 1)


def parse_optional_float(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    if not math.isfinite(number):
        return None
    return number


def finite_positive(value: float | None) -> bool:
    return value is not None and math.isfinite(value) and value > 0.0


def append_once(values: list[str], value: str) -> None:
    if value not in values:
        values.append(value)


def write_report(
    report: dict[str, Any],
    *,
    output: dict[str, Any] | None = None,
    quality_flags: list[str] | None = None,
    errors: list[str] | None = None,
) -> int:
    report["output"] = output
    report["quality_flags"] = quality_flags or []
    report["errors"] = errors or []
    json.dump(report, sys.stdout, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
