#!/usr/bin/env python3
"""Optional independent verification of distributed CLI metrics on retained data.

Original GreenLens research code, Apache-2.0. Standard library only; this is not
an application runtime dependency and never requests a simulation.
"""
import argparse
import hashlib
import json
import math
import struct
import subprocess
from datetime import datetime, timezone
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n", encoding="utf-8", newline="\n")


def inventory(project):
    # Writer lock and exported packages are outside the immutable run inventory.
    return {p.relative_to(project).as_posix(): digest(p)
            for p in sorted((project / "runs").rglob("*")) if p.is_file()}


def finite_metric_rows(rows, fields):
    for row in rows:
        for field in fields:
            value = row[field]
            if type(value) not in (int, float) or not math.isfinite(value) or not 0 <= value <= 100:
                raise ValueError("CLI metric must be a finite percentage in [0,100]")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("project", "cli", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--run", required=True)
    args = parser.parse_args()
    if args.output.exists() or Path(args.run).name != args.run or any(c in args.run for c in "/\\:"):
        raise ValueError("Use a new output directory and a single safe run identifier")
    record = json.loads((args.project / "runs" / args.run / "results.json").read_bytes())
    manifest = record["series"]
    count = len(manifest["sensor_ids"])
    if manifest["interval_count"] != 8760 or not 1 <= count <= 256:
        raise ValueError("Require a complete supported annual series")
    if (manifest["encoding"], manifest["ordering"], manifest["channels"]) != (
            "float64_little_endian", "time_sensor_channel", ["R", "G", "B", "illuminance"]):
        raise ValueError("Unsupported reference data convention")
    settings = [
        ("default", {"da_threshold_lux": 300, "udi_lower_lux": 100, "udi_upper_lux": 3000,
                     "occupancy": {"kind": "daily", "start_minute": 480, "end_minute": 1080}}),
        ("revised_daily", {"da_threshold_lux": 500, "udi_lower_lux": 200, "udi_upper_lux": 2000,
                           "occupancy": {"kind": "daily", "start_minute": 540, "end_minute": 1020}}),
        ("weighted_day_and_night", {"da_threshold_lux": 500, "udi_lower_lux": 200, "udi_upper_lux": 2000,
                                    "occupancy": {"kind": "mask", "weights": [0.5 if t == 0 else 1.0 if t == 1908 else 0.0 for t in range(8760)]}}),
    ]
    args.output.mkdir(parents=True)
    (args.output / "inspector-source.py").write_bytes(Path(__file__).read_bytes())
    before = inventory(args.project)
    project_hash = digest(args.project / "project.json")
    cli_hash = digest(args.cli)
    save(args.output / "declaration.json", {
        "schema_version": "greenlens.derived-metric-verification.v1", "declared_utc": datetime.now(timezone.utc).isoformat(),
        "run_id": args.run, "sensor_count": count, "intervals": 8760, "cli_sha256": cli_hash,
        "record_sha256": digest(args.project / "runs" / args.run / "results.json"),
        "inspector_sha256": digest(Path(__file__)), "cases": [name for name, _ in settings],
        "tolerance_percentage_points": 1e-9, "scope": "Derived metric arithmetic and immutable CLI reads; no engine accuracy or native UI acceptance"})
    fields = ("da_percent", "cda_percent", "udi_percent", "underlit_percent", "overlit_percent")
    results = []
    for name, setting in settings:
        occupancy = setting["occupancy"]
        weights = occupancy.get("weights", [1.0 if occupancy.get("start_minute", 0) <= t % 24 * 60 < occupancy.get("end_minute", 0) else 0.0 for t in range(8760)])
        denominator = math.fsum(weights)
        partials = [[[] for _ in fields] for _ in range(count)]
        next_interval = 0
        for index, chunk in enumerate(manifest["chunks"]):
            if chunk["file"] != f"data-{index:05}.f64" or chunk["start"] != next_interval:
                raise ValueError("Unexpected chunk path or order")
            path = args.project / "runs" / args.run / "series" / chunk["file"]
            data = path.read_bytes()
            if len(data) != chunk["bytes"] or len(data) != chunk["intervals"] * count * 32 or hashlib.sha256(data).hexdigest() != chunk["sha256"]:
                raise ValueError("Chunk shape or integrity failed")
            terms = [[[] for _ in fields] for _ in range(count)]
            for row, (_r, _g, _b, lux) in enumerate(struct.iter_unpack("<dddd", data)):
                interval, sensor = next_interval + row // count, row % count
                if not math.isfinite(lux) or lux < 0 or (interval == 0 and lux != 0):
                    raise ValueError("Invalid lux or declared occupied-night fixture is not zero")
                w = weights[interval]
                t, lower, upper = (setting[k] for k in ("da_threshold_lux", "udi_lower_lux", "udi_upper_lux"))
                values = (lux >= t, min(lux / t, 1), lower <= lux <= upper, lux < lower, lux > upper)
                for field, value in enumerate(values):
                    terms[sensor][field].append(w * value)
            for sensor in range(count):
                for field in range(5):
                    partials[sensor][field].append(math.fsum(terms[sensor][field]))
            next_interval += chunk["intervals"]
        if next_interval != 8760:
            raise ValueError("Truncated annual data")
        expected = [{field: 100 * math.fsum(partials[sensor][i]) / denominator for i, field in enumerate(fields)} for sensor in range(count)]
        source = args.output / (name + "-settings.json")
        save(source, setting)
        run = subprocess.run([str(args.cli), "study", "metrics", str(args.project), args.run, str(source)], capture_output=True, timeout=90)
        (args.output / (name + "-stdout.json")).write_bytes(run.stdout)
        (args.output / (name + "-stderr.txt")).write_bytes(run.stderr)
        if run.returncode != 0:
            raise ValueError("Distributed CLI metric read failed; inspect retained stderr")
        actual = json.loads(run.stdout)
        if actual["run_id"] != args.run or actual["settings"] != setting or len(actual["metrics"]) != count:
            raise ValueError("Derived identity, settings or shape mismatch")
        finite_metric_rows(actual["metrics"], fields)
        errors = [abs(a[field] - b[field]) for a, b in zip(actual["metrics"], expected) for field in fields]
        partition_error = max(abs(math.fsum(row[k] for k in fields[2:]) - 100) for row in actual["metrics"])
        if max(errors) > 1e-9 or partition_error > 1e-9:
            raise ValueError("Frozen metric arithmetic or UDI partition tolerance failed")
        results.append({"case": name, "metric_identity": actual["metric_identity"], "occupied_hour_equivalents": denominator,
                        "checks": len(errors), "maximum_error_pp": max(errors), "maximum_partition_error_pp": partition_error,
                        "reference": expected, "stdout_sha256": digest(args.output / (name + "-stdout.json")), "passed": True})
    empty = args.output / "empty-settings.json"
    save(empty, {**settings[0][1], "occupancy": {"kind": "mask", "weights": [0] * 8760}})
    rejection = subprocess.run([str(args.cli), "study", "metrics", str(args.project), args.run, str(empty)], capture_output=True, timeout=90)
    (args.output / "empty.stderr.txt").write_bytes(rejection.stderr)
    if rejection.returncode == 0 or b"occupied interval" not in rejection.stderr:
        raise ValueError("Empty occupied-period rejection missing")
    unchanged = before == inventory(args.project) and project_hash == digest(args.project / "project.json") and cli_hash == digest(args.cli)
    if not unchanged or len({r["metric_identity"] for r in results}) != 3:
        raise ValueError("Immutable data changed or metric identities were reused")
    report = {"schema_version": "greenlens.derived-metric-verification-result.v1", "passed": True,
              "declaration_sha256": digest(args.output / "declaration.json"), "cases": results,
              "empty_occupancy_rejected": True, "distinct_metric_identities": True,
              "run_files_unchanged": len(before), "project_and_cli_unchanged": unchanged, "release_acceptance_complete": False}
    save(args.output / "result.json", report)
    save(args.output / "checksums.json", {p.name: digest(p) for p in args.output.iterdir() if p.is_file()})
    print(json.dumps({"passed": True, "checks": sum(r["checks"] for r in results), "maximum_error_pp": max(r["maximum_error_pp"] for r in results), "run_files_unchanged": len(before)}))


if __name__ == "__main__":
    main()
