"""Original fixed interzone diagnostic; development only, no application runtime."""
import argparse
import csv
import hashlib
import importlib.util
import json
import math
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROTOCOL = HERE / "energy-interzone-protocol.json"


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def number(token):
    token=token.strip()
    assert 0<len(token)<=64 and re.fullmatch(r"[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?",token)
    value=float(token)
    assert math.isfinite(value)
    assert value!=0 or not any(c in "123456789" for c in re.split("[eE]",token)[0])
    return value


def prepare(root):
    p = json.loads(PROTOCOL.read_text())
    spec = importlib.util.spec_from_file_location("gate0", HERE / "energy-gate0-fixture.py")
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    declaration = base.make("envelope", root)
    m = json.loads((root / "model.epJSON").read_text())
    for kind in ("Zone", "BuildingSurface:Detailed", "SurfaceProperty:ConvectionCoefficients",
                 "ThermostatSetpoint:SingleCooling", "ZoneControl:Thermostat",
                 "ZoneHVAC:EquipmentConnections", "ZoneHVAC:EquipmentList", "ZoneHVAC:IdealLoadsAirSystem",
                 "Output:Variable"):
        m.pop(kind, None)
    def add(kind, name, **fields):
        m.setdefault(kind, {})[name] = fields
    m["Schedule:Constant"]["Control"]["hourly_value"] = 3
    pairs = {a:b for pair in p["pairs"] for a,b in ((pair["first"],pair["second"]),(pair["second"],pair["first"]))}
    for zone in p["zones"]:
        name = zone["id"]
        x,y,z = zone["origin_m"]
        w,d,h = zone["size_m"]
        add("Zone", name, direction_of_relative_north=0, x_origin=0,y_origin=0,z_origin=0,
            type=1,multiplier=1,ceiling_height=h,volume=w*d*h,floor_area=w*d)
        faces = {
            "South":("Wall",[(x,y,z+h),(x,y,z),(x+w,y,z),(x+w,y,z+h)]),
            "North":("Wall",[(x+w,y+d,z+h),(x+w,y+d,z),(x,y+d,z),(x,y+d,z+h)]),
            "West":("Wall",[(x,y+d,z+h),(x,y+d,z),(x,y,z),(x,y,z+h)]),
            "East":("Wall",[(x+w,y,z+h),(x+w,y,z),(x+w,y+d,z),(x+w,y+d,z+h)]),
            "Floor":("Floor",[(x,y,z),(x,y+d,z),(x+w,y+d,z),(x+w,y,z)]),
            "Roof":("Roof",[(x,y+d,z+h),(x,y,z+h),(x+w,y,z+h),(x+w,y+d,z+h)])}
        for face,(kind,vertices) in faces.items():
            key = f"{name}-{face}"
            fields = dict(surface_type=kind,construction_name="Wall",zone_name=name,
                outside_boundary_condition="Surface" if key in pairs else "Adiabatic",
                sun_exposure="NoSun",wind_exposure="NoWind",view_factor_to_ground=.5,
                number_of_vertices=4,vertices=[dict(vertex_x_coordinate=a,vertex_y_coordinate=b,vertex_z_coordinate=c) for a,b,c in vertices])
            if key in pairs:
                fields["outside_boundary_condition_object"] = pairs[key]
                add("Output:Variable", key, key_value=key,variable_name="Surface Inside Face Conduction Heat Transfer Rate",reporting_frequency="Timestep")
            add("BuildingSurface:Detailed", key, **fields)
            add("SurfaceProperty:ConvectionCoefficients", key+" Film",surface_name=key,
                convection_coefficient_1_location="Inside",convection_coefficient_1_type="Value",convection_coefficient_1=p["construction"]["inside_film_w_m2k"])
        add("Schedule:Constant", name+" Temperature",schedule_type_limits_name="Temperature",hourly_value=zone["target_c"])
        add("ThermostatSetpoint:SingleHeatingOrCooling",name+" Setpoint",setpoint_temperature_schedule_name=name+" Temperature")
        add("ZoneControl:Thermostat",name+" Thermostat",zone_or_zonelist_name=name,control_type_schedule_name="Control",
            control_1_object_type="ThermostatSetpoint:SingleHeatingOrCooling",control_1_name=name+" Setpoint")
        add("ZoneHVAC:EquipmentConnections",name+" Connections",zone_name=name,zone_conditioning_equipment_list_name=name+" List",
            zone_air_inlet_node_or_nodelist_name=name+" Supply",zone_air_exhaust_node_or_nodelist_name=name+" Return",zone_air_node_name=name+" Air")
        add("ZoneHVAC:EquipmentList",name+" List",load_distribution_scheme="SequentialLoad",equipment=[dict(zone_equipment_object_type="ZoneHVAC:IdealLoadsAirSystem",
            zone_equipment_name=name+" Ideal",zone_equipment_cooling_sequence=1,zone_equipment_heating_or_no_load_sequence=1)])
        add("ZoneHVAC:IdealLoadsAirSystem",name+" Ideal",availability_schedule_name="On",zone_supply_air_node_name=name+" Supply",zone_exhaust_air_node_name=name+" Return",
            maximum_heating_supply_air_temperature=50,minimum_cooling_supply_air_temperature=13,heating_limit="NoLimit",cooling_limit="NoLimit",
            heating_availability_schedule_name="On",cooling_availability_schedule_name="On",dehumidification_control_type="None",humidification_control_type="None")
        add("Output:Variable",name+" Temperature",key_value=name,variable_name="Zone Mean Air Temperature",reporting_frequency="Timestep")
    (root / "model.epJSON").write_text(json.dumps(m,indent=2,allow_nan=False)+"\n")
    declaration.update(case="interzone",zones=4,protocol_sha256=digest(PROTOCOL),
        generator_sha256=digest(Path(__file__)),base_generator_sha256=digest(HERE/"energy-gate0-fixture.py"),
        oracle=p["oracles"],native_limits={k:v for k,v in p["limits"].items() if k!="workers"})
    declaration["input_sha256"]["model.epJSON"] = digest(root/"model.epJSON")
    (root/"fixture-declaration.json").write_text(json.dumps(declaration,indent=2)+"\n")
    return declaration


def audit(root, output):
    p = json.loads(PROTOCOL.read_text())
    d = json.loads((root/"fixture-declaration.json").read_text())
    assert d["protocol_sha256"] == digest(PROTOCOL)
    assert d["generator_sha256"]==digest(Path(__file__)) and d["base_generator_sha256"]==digest(HERE/"energy-gate0-fixture.py")
    assert all(digest(root/k)==v for k,v in d["input_sha256"].items())
    process = json.loads((root/"processes.json").read_text(encoding="utf-8-sig"))
    r = process
    assert r["exit_code"]==0 and r["completed"] and r["job_empty"] and r["stream_tasks_completed"] and r["terminated_tree_reason"] is None
    execution=json.loads((root/"execution-declaration.json").read_text(encoding="utf-8-sig"))
    assert execution["fixture_sha256"]==digest(root/"fixture-declaration.json") and r["arguments"]==p["arguments"]==execution["arguments"]
    assert execution["dependency_sha256"]==digest(HERE.parent/"dependencies/energyplus.json")
    expected_sources={name:digest(HERE/name) for name in ("energy-gate0-run.ps1","energy-gate0-fixture.py","annual-paired-process.ps1","annual-paired-native-process.cs")}
    assert execution["source_sha256"]==expected_sources and execution["execute"] is True and execution["native_process_count"]==1
    assert execution["limits"]==d["native_limits"]=={k:v for k,v in p["limits"].items() if k!="workers"}
    assert all(r["limits"][k]==v for k,v in d["native_limits"].items())
    assert 0<=r["wall_seconds"]<=p["limits"]["timeout_seconds"]
    assert 0<=r["sampled_runner_plus_engine_peak_bytes"]<=p["limits"]["memory_bytes"]
    assert 0<=r["sampled_scratch_peak_bytes"]<=p["limits"]["scratch_bytes"]
    files=[f for f in root.iterdir() if f.is_file()]
    assert all(f.stat().st_size<=p["limits"]["file_bytes"] for f in files) and sum(f.stat().st_size for f in files)<=p["limits"]["scratch_bytes"]
    err=(root/"eplusout.err").read_text();end=(root/"eplusout.end").read_text()
    assert "Completed Successfully" in end and "0 Severe Errors" in end
    assert "** Severe **" not in err and "**  Fatal  **" not in err
    csvpath=root/"eplusout.csv"
    assert csvpath.stat().st_size <= p["limits"]["file_bytes"]
    data=[]
    with csvpath.open(newline="") as f:
        reader=csv.DictReader(f)
        expected={f'{z["id"]}:Zone Mean Air Temperature [C](TimeStep)' for z in p["zones"]}
        expected|={f'{face.upper()}:Surface Inside Face Conduction Heat Transfer Rate [W](TimeStep)' for pair in p["pairs"] for face in (pair["first"],pair["second"])}
        assert set(reader.fieldnames)==expected|{"Date/Time"} and len(reader.fieldnames)==len(expected)+1
        for i,row in enumerate(reader):
            assert i<1008 and row["Date/Time"].strip()==f'01/{i//144+1:02}  {(i%144+1)//6:02}:{((i%144+1)%6)*10:02}:00'
            values={k:number(row[k]) for k in expected}
            assert all(math.isfinite(v) for v in values.values())
            data.append(values)
    assert len(data)==1008
    tail=data[-144:];checks=[];faces=[]
    for pair in p["pairs"]:
        q=pair["area_m2"]*pair["delta_temperature_k"]/(p["construction"]["resistance_m2k_w"]+2/p["construction"]["inside_film_w_m2k"])
        keys=[f'{face.upper()}:Surface Inside Face Conduction Heat Transfer Rate [W](TimeStep)' for face in (pair["first"],pair["second"])]
        faces+=keys
        for k,target in zip(keys,[q,-q]):
            v=[r[k] for r in tail];error=max(abs(x-target) for x in v);span=max(v)-min(v)
            checks.append(dict(id=k,expected_w=target,mean_w=math.fsum(v)/144,max_error_w=error,range_w=span,passed=error<=max(1,.01*abs(target)) and span<=.01))
        residual=max(abs(r[keys[0]]+r[keys[1]]) for r in tail)
        checks.append(dict(id=pair["first"]+" + "+pair["second"],max_abs_w=residual,passed=residual<=.01))
    net=max(abs(math.fsum(r[k] for k in faces)) for r in tail)
    checks.append(dict(id="all eight paired faces",max_abs_w=net,passed=net<=.01))
    for z in p["zones"]:
        error=max(abs(r[f'{z["id"]}:Zone Mean Air Temperature [C](TimeStep)']-z["target_c"]) for r in tail)
        checks.append(dict(id=z["id"]+" temperature",max_error_c=error,passed=error<=.00001))
    result=dict(schema="greenlens.energy-interzone-audit.v1",protocol_sha256=digest(PROTOCOL),
        source_sha256=digest(Path(__file__)),rows=len(data),checks=checks,fixture_passed=all(c["passed"] for c in checks),
        qualification_asserted=False,process=process,files={f.name:{"sha256":digest(f),"bytes":f.stat().st_size} for f in root.iterdir() if f.is_file()})
    output.write_text(json.dumps(result,indent=2,allow_nan=False)+"\n")
    return {"fixture_passed":result["fixture_passed"],"checks":len(checks),"sha256":digest(output)}


if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("--root",type=Path,required=True);parser.add_argument("--audit",type=Path)
    args=parser.parse_args();print(json.dumps(audit(args.root,args.audit) if args.audit else prepare(args.root)))
