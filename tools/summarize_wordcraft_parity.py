#!/usr/bin/env python3
"""Save a compact, provenance-linked before/after snapshot without dropping failed cases."""
import argparse
import hashlib
import json
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def issues(case):
    result = set()
    for error in case.get("errors", []):
        result.update(error if isinstance(error, dict) else [error])
    for page in case.get("pages", []):
        for error in page.get("errors", []):
            result.update(error if isinstance(error, dict) else [error])
    if case.get("render_errors"):
        result.add("render_errors")
    if case.get("missing_evidence"):
        result.add("missing_evidence")
    if case.get("error"):
        result.add(case["error"])
    return sorted(result)


def summarize(directory):
    path = directory / "comparison.json"
    report = read(path)
    manifest = read(directory / "manifest.json")
    entries = {entry["id"]: entry for entry in manifest["cases"]}
    cases = {case["id"]: case for case in report["cases"]}
    if cases.keys() != entries.keys():
        raise ValueError("Report does not contain every manifest case")
    accepted = [page for case in cases.values() if case["status"] == "passed" for page in case["pages"]]
    fonts = next((read(directory / entry["id"] / "edgedoc/layout.json")["fonts"]
                  for entry in entries.values() if (directory / entry["id"] / "edgedoc/layout.json").exists()), [])
    return {
        "report_path": str(path), "report_sha256": sha256(path),
        "manifest_sha256": sha256(directory / "manifest.json"),
        "passed": report["passed"], "counts": dict(Counter(case["status"] for case in cases.values())),
        "external_counts": dict(Counter(case["status"] for key, case in cases.items() if entries[key].get("source"))),
        "distinct_docx_inputs": len({entry["input_sha256"] for entry in entries.values()}),
        "reference": report["reference"],
        "reference_reused_cases": report.get("reference_reused_cases", []),
        "edgedoc_build_inputs_sha256": report["edgedoc_build_inputs_sha256"],
        "comparison_tool_sha256": report["comparison_tool_sha256"], "thresholds": report["thresholds"],
        "supplied_fonts": fonts,
        "passing_pages": len(accepted),
        "maximum_passing_glyph_origin_error_pt": max((p["max_glyph_origin_error_pt"] for p in accepted), default=None),
        "maximum_passing_unmatched_ink_fraction": max((p["unmatched_ink_fraction"] for p in accepted), default=None),
        "maximum_passing_ink_color_error": max((p["mean_ink_color_error"] for p in accepted), default=None),
        "cases": [{**entry, "status": cases[key]["status"], "issues": issues(cases[key]),
                   "compared_pages": len(cases[key].get("pages", [])),
                   "failed_pages": sum(not p["passed"] for p in cases[key].get("pages", [])),
                   "diagnostics": cases[key].get("diagnostics", []),
                   "reference_adapter_limits": sorted({str(limit) for error in cases[key].get("errors", [])
                       if isinstance(error, dict) for limit in error.get("reference_adapter_limit", [])}),
                   "render_errors": cases[key].get("render_errors", [])}
                  for key, entry in entries.items()],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    before, after = summarize(args.before), summarize(args.after)
    current = {case["id"]: case for case in after["cases"]}
    for case in before["cases"]:
        if case["id"] not in current or current[case["id"]]["input_sha256"] != case["input_sha256"]:
            raise ValueError(f"Prior input missing or modified: {case['id']}")
    transitions = Counter(f"{case['status']} -> {current[case['id']]['status']}" for case in before["cases"])
    snapshot = {"schema": 1, "checked_at": datetime.now(timezone.utc).isoformat(),
                "scope": "Bounded DOCX fixture evidence against Wordcraft. Full rendering parity remains unproven.",
                "all_prior_inputs_retained": True, "retained_case_transitions": dict(transitions),
                "measurement_note": "The reference engine is unchanged. The adapter gained image/label export, so inspect both case-level failures and provenance rather than attributing every transition to product changes.",
                "before": before, "after": after}
    args.output.write_text(json.dumps(snapshot, ensure_ascii=False, indent=2) + "\n")
    print(f"Saved {len(after['cases'])} case records, including every failure: {args.output}")


if __name__ == "__main__":
    main()
