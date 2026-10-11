#!/usr/bin/env python3
"""Predeclared Hammersley cosine rule on the retained black-aperture fixtures.

Read-only baseline evidence; imports only the repository-owned geometry helper,
never any retained source/command. No engines or production settings change.
"""
import argparse
import hashlib
import importlib.util
import json
import math
from datetime import datetime, timezone
from pathlib import Path

COUNTS = (16384, 65536, 262144)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False)+"\n", encoding="utf-8", newline="\n")


def radical_inverse(index):
    value, fraction = 0.0, 0.5
    while index:
        value += (index & 1)*fraction
        fraction *= 0.5
        index >>= 1
    return value


def directions(count):
    for index in range(count):
        radius = math.sqrt((index+.5)/count)
        phi = 2*math.pi*radical_inverse(index)
        yield radius*math.cos(phi), radius*math.sin(phi), math.sqrt(1-radius*radius)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError("Use a new output directory; prior evidence is immutable")
    seals = json.loads((args.baseline/"checksums.json").read_text(encoding="utf-8-sig"))
    for name, expected in seals.items():
        if Path(name).name != name or not (args.baseline/name).is_file() or (args.baseline/name).stat().st_size > 1024*1024 or digest(args.baseline/name) != expected:
            raise ValueError("Baseline evidence integrity/path/bound check failed")
    original = json.loads((args.baseline/"declaration.json").read_text(encoding="utf-8-sig"))
    baseline = json.loads((args.baseline/"aperture-quadrature.json").read_text(encoding="utf-8-sig"))
    if baseline["declaration_sha256"] != digest(args.baseline/"declaration.json") or not baseline["completed"] or not baseline["oracle_converged"]:
        raise ValueError("Require complete converged aperture-oracle evidence")
    trusted = Path(__file__).with_name("annual-aperture-quadrature.py")
    if digest(trusted) != original["inspector_sha256"]:
        raise ValueError("Repository geometry/oracle helper differs from the retained baseline")
    spec = importlib.util.spec_from_file_location("greenlens_aperture_oracle", trusted)
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    models, sensors = original["models"], original["sensors"]
    if len(sensors) != 24 or original["exterior_horizontal_illuminance_lux"] != 10000 or original["effective_sensor_origin_offset_m"] != 1e-4:
        raise ValueError("Unexpected baseline fixture convention")
    expected = {(a,f"s{i+1:06d}",sky) for a in ("A","B") for i in range(24) for sky in ("uniform","overcast")}
    keys = [(r["alternative"],r["sensor_id"],r["sky"]) for r in baseline["rows"]]
    if len(keys) != 96 or set(keys) != expected:
        raise ValueError("Baseline does not have exact96-case coverage")
    references = {key:row["reference_lux"] for key,row in zip(keys,baseline["rows"])}
    args.output.mkdir(parents=True)
    declaration = {"schema_version":"greenlens.aperture-hammersley-research.v1","declared_utc":datetime.now(timezone.utc).isoformat(),"scope":"Direct black/unglazed aperture visibility only; no reflected or annual accuracy claim","baseline_declaration_sha256":digest(args.baseline/"declaration.json"),"baseline_result_sha256":digest(args.baseline/"aperture-quadrature.json"),"baseline_seal_sha256":digest(args.baseline/"checksums.json"),"trusted_geometry_helper_sha256":digest(trusted),"inspector_sha256":digest(__file__),"sample_counts":COUNTS,"radial_rule":"sqrt((i+0.5)/N)","azimuth_rule":"2*pi*radical_inverse_base2(i)","vertical_rule":"sqrt(1-radius*radius)","sequence":"i=0..N-1, no rotation, no scramble, same sequence at each sensor","weight":"pi/N per primary for cosine-weighted hemisphere","diagnostic_band":"max(5 lx,5% independent reference), unchanged","case_count":96,"oracle_relative_tolerance":.001,"effective_origin_offset_m":1e-4,"fixture_overrides":original["fixture_overrides"],"release_acceptance_complete":False}
    save(args.output/"declaration.json",declaration)
    (args.output/"inspector-source.py").write_bytes(Path(__file__).read_bytes())
    if args.prepare_only:
        print("Predeclared only; Hammersley checks not executed")
        return
    try:
        if [radical_inverse(i) for i in range(8)] != [0,.5,.25,.75,.125,.625,.375,.875]:
            raise ValueError("Base2 radical-inverse self-check failed")
        all_rows = []
        for count in COUNTS:
            rays = [ray for ray in directions(count) if ray[1] < 0]
            for i,sensor in enumerate(sensors):
                for alternative,model in models.items():
                    visible = 0
                    luminance_sum = 0.0
                    for direction in rays:
                        if oracle.intersects(sensor,direction,model):
                            visible += 1
                            luminance_sum += 1+2*direction[2]
                    values = (10000*visible/count, 3*10000/7*luminance_sum/count)
                    for sky,value in zip(("uniform","overcast"),values):
                        key = (alternative,f"s{i+1:06d}",sky)
                        ref = references[key]
                        difference = value-ref
                        all_rows.append({"alternative":key[0],"sensor_id":key[1],"sky":key[2],"primaries":count,"visible_primaries":visible,"reference_lux":ref,"lux":value,"signed_difference_lux":difference,"relative_error":difference/ref,"allowed_lux":max(5,.05*ref),"passed":abs(difference)<=max(5,.05*ref)})
        summary = {}
        for count in COUNTS:
            rows = [row for row in all_rows if row["primaries"] == count]
            summary[str(count)]={"checks":len(rows),"failed_count":sum(not row["passed"] for row in rows),"maximum_absolute_error_lux":max(abs(row["signed_difference_lux"]) for row in rows),"maximum_absolute_relative_error":max(abs(row["relative_error"]) for row in rows),"worst_relative_case":max(rows,key=lambda row:abs(row["relative_error"])),"failed_cases":[row for row in rows if not row["passed"]]}
        result = {"schema_version":"greenlens.aperture-hammersley-results.v1","completed":True,"declaration_sha256":digest(args.output/"declaration.json"),"scope":declaration["scope"],"radical_inverse_self_check_passed":True,"oracle_reused_from_verified_baseline":True,"summary":summary,"rows":all_rows,"release_acceptance_complete":False}
        save(args.output/"aperture-hammersley.json",result)
        save(args.output/"checksums.json",{p.name:digest(p) for p in args.output.iterdir() if p.is_file()})
        print(json.dumps({key:{field:value for field,value in row.items() if field not in ("failed_cases","worst_relative_case")} for key,row in summary.items()},indent=2))
    except Exception as error:
        save(args.output/"failure.json",{"completed":False,"error":str(error),"release_acceptance_complete":False})
        raise


if __name__ == "__main__":
    main()
