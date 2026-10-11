#!/usr/bin/env python3
"""Research-only direct-aperture quadrature; Python is not an app dependency.

No engines, generated commands, production settings or candidate arrays run.
The aperture-area oracle integrates x analytically, then z by Gauss-Legendre
quadrature split at every projected-overhang clipping boundary. The other
integrator reproduces the fixed cosine-weighted hemisphere midpoint rule.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
from datetime import datetime, timezone

SIDES = (128, 256, 512)
ORDERS = (16, 32, 64)
EXTERIOR_LUX = 10000.0
ORIGIN_OFFSET = 1e-4


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")


def gauss(n):
    """Legendre roots/weights on [-1,1], without third-party packages."""
    result = []
    for i in range(1, (n + 1) // 2 + 1):
        z = math.cos(math.pi * (i - .25) / (n + .5))
        for _ in range(30):
            p0, p1 = 1.0, z
            for k in range(2, n + 1):
                p0, p1 = p1, ((2*k-1)*z*p1-(k-1)*p0)/k
            derivative = n * (z*p1-p0)/(z*z-1)
            next_z = z-p1/derivative
            if abs(next_z-z) < 2e-16:
                z = next_z
                break
            z = next_z
        else:
            raise ValueError("Legendre root did not converge")
        # Re-evaluate derivative at the converged root.
        p0, p1 = 1.0, z
        for k in range(2, n + 1):
            p0, p1 = p1, ((2*k-1)*z*p1-(k-1)*p0)/k
        derivative = n*(z*p1-p0)/(z*z-1)
        weight = 2/((1-z*z)*derivative*derivative)
        result.extend(((-z, weight), (z, weight)))
    return sorted(result)


def aperture(model):
    w = model["window"]
    return (w["center_x_m"]-w["width_m"]/2,
            w["center_x_m"]+w["width_m"]/2,
            w["sill_m"], w["sill_m"]+w["height_m"])


def shading(model):
    o = model["overhang"]
    if o is None:
        return None
    left, right, _, top = aperture(model)
    return (left-o["side_extension_m"], right+o["side_extension_m"],
            o["projection_m"], top+o["gap_above_window_m"])


def primitive2(u, c2):
    # Integral (u²+c²)^(-2) du.
    return u/(2*c2*(u*u+c2)) + math.atan(u/math.sqrt(c2))/(2*c2**1.5)


def primitive5(u, c2):
    # Integral (u²+c²)^(-5/2) du.
    return u*(3*c2+2*u*u)/(3*c2*c2*(u*u+c2)**1.5)


def x_integral(left, right, height, sensor):
    """Integral over a horizontal aperture strip, for both prescribed skies."""
    sx, sy, sz = sensor
    dz = height-sz
    if right <= left or dz <= 0:
        return (0.0, 0.0)
    c2 = sy*sy+dz*dz
    j2 = primitive2(right-sx, c2)-primitive2(left-sx, c2)
    j5 = primitive5(right-sx, c2)-primitive5(left-sx, c2)
    # E = integral L(omega) * omega_z * |omega_y| / r² dx dz.
    uniform = EXTERIOR_LUX/math.pi * sy*dz*j2
    overcast = 3*EXTERIOR_LUX/(7*math.pi) * sy*(dz*j2+2*dz*dz*j5)
    return uniform, overcast


def shadow_interval(height, sensor, model):
    """Projected overhang interval on y=0 at this z; independent of ray grid."""
    overhang = shading(model)
    if overhang is None:
        return None
    sx, sy, sz = sensor
    left, right, depth, top = overhang
    dz, rise = height-sz, top-sz
    if dz <= 0 or rise <= 0 or dz < rise*sy/(sy+depth) or dz > rise:
        return None
    ratio = dz/rise
    return sx+(left-sx)*ratio, sx+(right-sx)*ratio


def oracle(model, sensor, order):
    left, right, bottom, top = aperture(model)
    bottom = max(bottom, sensor[2])
    breaks = {bottom, top}
    overhang = shading(model)
    if overhang is not None:
        sx, sy, sz = sensor
        ol, oright, depth, height = overhang
        rise = height-sz
        breaks.add(sz+rise*sy/(sy+depth))
        breaks.add(height)
        # Every switch of max(aperture_left,projected_left) and
        # min(aperture_right,projected_right) has an explicit split.
        for edge in (ol, oright):
            if edge != sx:
                for window_edge in (left, right):
                    breaks.add(sz+rise*(window_edge-sx)/(edge-sx))
    breaks = sorted(x for x in breaks if bottom <= x <= top)
    totals = [0.0, 0.0]
    nodes = gauss(order)
    for low, high in zip(breaks, breaks[1:]):
        for node, weight in nodes:
            height = (low+high)/2 + node*(high-low)/2
            values = x_integral(left, right, height, sensor)
            blocked = shadow_interval(height, sensor, model)
            if blocked is not None:
                obscured = x_integral(max(left, blocked[0]), min(right, blocked[1]), height, sensor)
                values = tuple(a-b for a, b in zip(values, obscured))
            for sky in range(2):
                totals[sky] += values[sky]*weight*(high-low)/2
    if any(not math.isfinite(v) or v <= 0 for v in totals):
        raise ValueError("Invalid independent aperture integral")
    return totals


def intersects(sensor, direction, model):
    """Ray/plane visibility used only by the hemisphere-grid estimator."""
    sx, sy, sz = sensor
    dx, dy, dz = direction
    if dy >= 0:
        return False
    t = -sy/dy
    x, z = sx+t*dx, sz+t*dz
    left, right, bottom, top = aperture(model)
    if not (left <= x <= right and bottom <= z <= top):
        return False
    overhang = shading(model)
    if overhang is not None and dz > 0:
        ol, oright, depth, height = overhang
        shade_t = (height-sz)/dz
        hx, hy = sx+shade_t*dx, sy+shade_t*dy
        if shade_t >= t and ol <= hx <= oright and -depth <= hy <= 0:
            return False
    return True


def hemisphere(models, sensors, side):
    # Same sensor-major radial/azimuth midpoint formula as the inspected C#
    # generator. 512 is a research-only extension of this rule, not a setting
    # accepted by that generator or the production application.
    results = {key: [] for key in models}
    phis = [2*math.pi*(i+.5)/side for i in range(side)]
    horizontal = [(math.cos(phi), math.sin(phi)) for phi in phis]
    radii = [math.sqrt((i+.5)/side) for i in range(side)]
    rows = [(radius, math.sqrt(1-radius*radius)) for radius in radii]
    for sensor in sensors:
        counts = {key: 0 for key in models}
        overcast_sums = {key: 0.0 for key in models}
        for radius, dz in rows:
            for cx, cy in horizontal:
                if cy >= 0:
                    continue
                direction = (radius*cx, radius*cy, dz)
                for key, model in models.items():
                    if intersects(sensor, direction, model):
                        counts[key] += 1
                        overcast_sums[key] += 1+2*dz
        for key in models:
            results[key].append({"visible_primary_count": counts[key],
                                 "uniform_lux": EXTERIOR_LUX*counts[key]/(side*side),
                                 "overcast_lux": 3*EXTERIOR_LUX/7*overcast_sums[key]/(side*side)})
    return results


def self_test():
    for n in ORDERS:
        assert abs(sum(w*x**6 for x, w in gauss(n))-2/7) < 2e-14
        assert abs(sum(w for _, w in gauss(n))-2) < 2e-14
    for u, c2 in ((.3, .8), (-1.4, 4.0), (3.0, 20.0)):
        h = 1e-5
        assert abs((primitive2(u+h, c2)-primitive2(u-h, c2))/(2*h)-(u*u+c2)**-2) < 1e-8
        assert abs((primitive5(u+h, c2)-primitive5(u-h, c2))/(2*h)-(u*u+c2)**-2.5) < 1e-8
    return {"kind": "pure mathematical implementation checks", "checks": 12, "passed": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("a-model", "b-model", "sensors", "generator", "output"):
        parser.add_argument("--"+name, required=True, type=Path)
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError("Use a new output directory; prior evidence is immutable")
    models = {"A": json.loads(args.a_model.read_text(encoding="utf-8-sig")),
              "B": json.loads(args.b_model.read_text(encoding="utf-8-sig"))}
    sensors = []
    for line in args.sensors.read_text(encoding="utf-8-sig").splitlines():
        numbers = tuple(map(float, line.split()))
        if len(numbers) != 6 or not all(map(math.isfinite, numbers)) or numbers[3:] != (0., 0., 1.):
            raise ValueError("Expected ordered upward sensor records")
        sensors.append((numbers[0], numbers[1], numbers[2]+ORIGIN_OFFSET))
    if len(sensors) != 24:
        raise ValueError("Expected all24 fixed room sensors")
    for key, model in models.items():
        if model["room"] != {"width_m":4,"depth_m":6,"height_m":3,"rotation_deg":0} or model["obstruction"] is not None:
            raise ValueError("Unexpected fixed room transform or context")
        if aperture(model) != (1.1,2.9,.9,2.4):
            raise ValueError("Unexpected fixed window")
        expected_shading = None if key == "A" else (1.1,2.9,.6,2.4)
        if shading(model) != expected_shading:
            raise ValueError("Unexpected fixed overhang")
    expected_sensors = [(.25+(x+.5)*3.5/4, .25+(y+.5)*5.5/6, .8+ORIGIN_OFFSET) for y in range(6) for x in range(4)]
    if any(abs(x-y)>1e-14 for a,b in zip(sensors,expected_sensors) for x,y in zip(a,b)):
        raise ValueError("Sensor positions/order differ from fixed equal-area grid")
    generator = args.generator.read_text(encoding="utf-8-sig")
    for expression in ("Math.Sqrt((radial+.5)/side)", "2*Math.PI*(azimuth+.5)/side", "p[2]+.0001"):
        if expression not in generator:
            raise ValueError("Inspected external cosine-grid generator has changed")
    args.output.mkdir(parents=True)
    declaration = {
        "schema_version":"greenlens.aperture-quadrature-research.v1",
        "declared_utc":datetime.now(timezone.utc).isoformat(),
        "scope":"Direct aperture visibility only; black opaque surfaces, unglazed opening, zero ground; no reflected or annual validation",
        "input_sha256":{"A-model.json":digest(args.a_model),"B-model.json":digest(args.b_model),"sensors.pts":digest(args.sensors),"inspected-primary-generator.cs":digest(args.generator)},
        "inspector_sha256":digest(__file__), "models":models,
        "fixture_overrides":{"all_opaque_reflectance":0,"glazing":"removed / clear unglazed aperture","ground_luminance":0},
        "effective_sensor_origin_offset_m":ORIGIN_OFFSET,"sensors":sensors,"sensor_order":"s000001 through s000024, x fastest",
        "exterior_horizontal_illuminance_lux":EXTERIOR_LUX,
        "sky_luminance":{"uniform":"E_out/pi","cie_overcast":"3*E_out*(1+2*omega_z)/(7*pi)"},
        "hemisphere_sides":SIDES,"bounded_extension":"512 uses same formula solely for research; actual inspected generator allows128/256",
        "independent_oracle":"aperture area integral; exact horizontal antiderivatives, Gauss-Legendre vertical quadrature split at projected-overhang clipping boundaries",
        "oracle_orders":ORDERS,"oracle_convergence_relative_tolerance":.001,
        "diagnostic_band":"max(5 lx, 5% independent reference) at every sensor; retained unchanged; this fixture alone is not annual acceptance",
        "method_limits":"Fixed upward horizontal sensors, planar south aperture and horizontal overhang; constant/azimuth-symmetric skies only; no glass, diffuse reflection, solar beam, weather or certification",
        "release_acceptance_complete":False,
    }
    save(args.output/"declaration.json", declaration)
    for name, source in (("A-model.json",args.a_model),("B-model.json",args.b_model),("sensors.pts",args.sensors),("inspected-primary-generator.cs",args.generator),("inspector-source.py",Path(__file__))):
        (args.output/name).write_bytes(source.read_bytes())
    if args.prepare_only:
        print("Predeclared only; numerical checks not executed")
        return
    try:
        pure_checks = self_test()
        references = {key:[{str(order):oracle(model,sensor,order) for order in ORDERS} for sensor in sensors] for key,model in models.items()}
        grids = {str(side):hemisphere(models,sensors,side) for side in SIDES}
        rows = []
        for key in models:
            for i,sensor in enumerate(sensors):
                for sky,sky_name in enumerate(("uniform","overcast")):
                    values = [references[key][i][str(n)][sky] for n in ORDERS]
                    convergence = [{"orders":[ORDERS[k],ORDERS[k+1]],"absolute_difference_lux":abs(values[k+1]-values[k]),"relative_difference":abs(values[k+1]-values[k])/values[k+1],"passed":abs(values[k+1]-values[k])/values[k+1] <= .001} for k in range(2)]
                    reference = values[-1]
                    comparisons = []
                    for side in SIDES:
                        g = grids[str(side)][key][i]
                        value = g[sky_name+"_lux"]
                        error = value-reference
                        comparisons.append({"side":side,"primaries":side*side,"visible_primary_count":g["visible_primary_count"],"lux":value,"signed_difference_lux":error,"relative_error":error/reference,"allowed_lux":max(5,.05*reference),"passed":abs(error)<=max(5,.05*reference)})
                    rows.append({"alternative":key,"sensor_id":f"s{i+1:06d}","effective_position_m":sensor,"sky":sky_name,"oracle_lux_by_order":dict(zip(map(str,ORDERS),values)),"oracle_convergence":convergence,"reference_lux":reference,"hemisphere":comparisons})
        summary = {}
        for side in SIDES:
            cases = [dict(alternative=r["alternative"],sensor_id=r["sensor_id"],sky=r["sky"],**next(c for c in r["hemisphere"] if c["side"] == side)) for r in rows]
            summary[str(side)]={"checks":len(cases),"failed_count":sum(not c["passed"] for c in cases),"maximum_absolute_error_lux":max(abs(c["signed_difference_lux"]) for c in cases),"maximum_absolute_relative_error":max(abs(c["relative_error"]) for c in cases),"worst_relative_case":max(cases,key=lambda c:abs(c["relative_error"])),"failed_cases":[c for c in cases if not c["passed"]]}
        result={"schema_version":"greenlens.aperture-quadrature-results.v1","completed":True,"declaration_sha256":digest(args.output/"declaration.json"),"pure_checks":pure_checks,"oracle_converged":all(c["passed"] for r in rows for c in r["oracle_convergence"]),"maximum_oracle_relative_change":max(c["relative_difference"] for r in rows for c in r["oracle_convergence"]),"summary":summary,"rows":rows,"release_acceptance_complete":False}
        save(args.output/"aperture-quadrature.json",result)
        save(args.output/"checksums.json",{p.name:digest(p) for p in args.output.iterdir() if p.is_file()})
        print(json.dumps({"oracle_converged":result["oracle_converged"],"maximum_oracle_relative_change":result["maximum_oracle_relative_change"],"summary":{k:{f:v for f,v in x.items() if f not in ("failed_cases","worst_relative_case")} for k,x in summary.items()}},indent=2))
        if not result["oracle_converged"]:
            raise ValueError("Independent oracle failed predeclared convergence; no diagnostic acceptance")
    except Exception as error:
        save(args.output/"failure.json",{"completed":False,"error":str(error),"release_acceptance_complete":False})
        raise


if __name__ == "__main__":
    main()
