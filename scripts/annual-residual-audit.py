"""Read-only numerical audit of retained failed method4 matrix products.
Development investigation only; no engine, product runtime or output repair.
"""
import argparse
import hashlib
import json
import math
import re
from datetime import datetime, timezone
from pathlib import Path


def matrix(path):
    raw = path.read_bytes()
    parts = re.split(rb"\r?\n\s*\r?\n", raw, maxsplit=1)
    if len(parts) != 2:
        raise ValueError("Missing matrix header terminator")
    header = parts[0].decode("ascii")
    shape = tuple(int(re.search(rf"(?m)^{key}=(\d+)\r?$", header)[1])
                  for key in ("NROWS", "NCOLS", "NCOMP"))
    values = [float(value) for value in parts[1].split()]
    if len(values) != math.prod(shape) or shape[2] != 3:
        raise ValueError("Wrong matrix shape/count")
    if any(not math.isfinite(value) or value < 0 for value in values):
        raise ValueError("Invalid original matrix value")
    return shape, values


def lux(rgb):
    return 179 * sum(a*b for a, b in zip(rgb, (.265074126, .670114631, .064811243)))


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
if args.output.exists():
    raise ValueError("Preserve previous evidence; choose new output")
root = args.directory
files = ["annual-coefficients.mtx", "annual-direct-coefficients.mtx",
         "annual-solar-patch-rgb.mtx", "annual-direct-patch-rgb.mtx",
         "annual-diffuse-rgb.mtx", "annual-sun-modifiers.txt",
         "annual-coefficient-receiver.rad", "annual-coefficient-sensors.pts",
         "annual-scene.rad", "annual-sensors.pts", "annual-logs.json"]
hashes = {name: hashlib.sha256((root/name).read_bytes()).hexdigest() for name in files}
receiver = (root/"annual-coefficient-receiver.rad").read_text()
if not re.search(r"\b4\s+100\s+100\s+100\s+0\b", receiver):
    raise ValueError("This audit requires the retained receiver scale100")
sun_indices = set()
for line in (root/"annual-sun-modifiers.txt").read_text().splitlines():
    match = re.fullmatch(r"solar(\d+)", line)
    if not match or (int(match[1])-30) % 60:
        raise ValueError("Invalid source-to-time mapping")
    sun_indices.add((int(match[1])-30)//60)
shape, full = matrix(root/"annual-solar-patch-rgb.mtx")
direct_shape, direct = matrix(root/"annual-direct-patch-rgb.mtx")
diffuse_shape, diffuse = matrix(root/"annual-diffuse-rgb.mtx")
if shape != direct_shape or shape != diffuse_shape or shape[:2] != (24, 8760):
    raise ValueError("Expected matching24x8760 products")
negative = []
all_negative_channels = 0
for sensor in range(shape[0]):
    for interval in range(shape[1]):
        offset = (sensor*shape[1]+interval)*3
        residual = [(full[offset+c]-direct[offset+c])/100 for c in range(3)]
        if min(residual) < 0:
            all_negative_channels += sum(v < 0 for v in residual)
            negative.append(dict(sensor_index=sensor, sensor_id=f"s{sensor+1:06d}",
                                 interval_index=interval, emitted_source=interval in sun_indices,
                                 full_rgb=[v/100 for v in full[offset:offset+3]],
                                 direct_rgb=[v/100 for v in direct[offset:offset+3]],
                                 reflected_rgb=residual, reflected_lux=lux(residual),
                                 diffuse_lux=lux([v/100 for v in diffuse[offset:offset+3]])))
coeff_shape, coeff = matrix(root/"annual-coefficients.mtx")
direct_coeff_shape, direct_coeff = matrix(root/"annual-direct-coefficients.mtx")
if coeff_shape != direct_coeff_shape or coeff_shape[:2] != (24, 578):
    raise ValueError("Expected matching24x578 coefficients")
negative_coeff = []
for sensor in range(24):
    for patch in range(578):
        offset = (sensor*578+patch)*3
        delta = [(coeff[offset+c]-direct_coeff[offset+c])/100 for c in range(3)]
        if min(delta) < 0:
            negative_coeff.append(dict(sensor_index=sensor, patch_index=patch,
                                       full_rgb=[v/100 for v in coeff[offset:offset+3]],
                                       direct_rgb=[v/100 for v in direct_coeff[offset:offset+3]],
                                       reflected_coefficient_rgb=delta))
active = [row for row in negative if row["emitted_source"]]
result = dict(schema_version="greenlens.method4-residual-audit.v1",
              created_utc=datetime.now(timezone.utc).isoformat(),
              scope="Read-only failed-candidate residual audit; no clamp, repair or acceptance",
              source_file_sha256=hashes, shape=shape, receiver_scale=100,
              emitted_solar_intervals=len(sun_indices),
              negative_sensor_hour_count_all=len(negative),
              negative_channel_count_all=all_negative_channels,
              negative_sensor_hour_count_emitted=len(active),
              minimum_reflected_lux=min((row["reflected_lux"] for row in active), default=0),
              maximum_negative_absolute_lux=max((-row["reflected_lux"] for row in active), default=0),
              negative_coefficient_sensor_patch_count=len(negative_coeff),
              worst_active_rows=sorted(active, key=lambda row: row["reflected_lux"])[:20],
              all_negative_rows=negative, all_negative_coefficients=negative_coeff,
              release_acceptance_complete=False)
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(result, indent=2, allow_nan=False)+"\n", encoding="utf-8")
print(json.dumps({key: value for key, value in result.items()
                  if key not in ("all_negative_rows", "all_negative_coefficients", "source_file_sha256")},
                 indent=2, allow_nan=False))
