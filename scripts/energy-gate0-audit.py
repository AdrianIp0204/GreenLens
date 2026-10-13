"""Read closed, declared Gate 0 CSVs; emit a compact path-free audit.

Development-only independent arithmetic. Does not execute EnergyPlus.
"""
import argparse
import csv
import datetime as dt
import hashlib
import json
import math
from decimal import Decimal
import re
import struct
from pathlib import Path
from pathlib import PurePosixPath


def sha(p):
    with p.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()


def audit(root):
    d=json.loads((root/"fixture-declaration.json").read_text(encoding="utf-8-sig"))
    execution=json.loads((root/"execution-declaration.json").read_text(encoding="utf-8-sig"))
    process=json.loads((root/"processes.json").read_text(encoding="utf-8-sig"))
    assert process["completed"] is True and process["exit_code"]==0 and process["job_empty"] is True and process["stream_tasks_completed"] is True and process["terminated_tree_reason"] is None
    assert process["arguments"]==execution["arguments"]
    assert execution["fixture_sha256"]==sha(root/"fixture-declaration.json")
    for n,h in d["input_sha256"].items():assert sha(root/n)==h
    error=(root/"eplusout.err").read_text()
    assert "** Severe" not in error and "**  Fatal" not in error
    assert "EnergyPlus Completed Successfully" in (root/"eplusout.end").read_text()
    results={}; checks={"finite_values":0,"timestamps":0,"meter_partition":0,"supply_ventilation_partition":0,"package_aggregate":0,"off_cycle":0}
    maximum_residual_j=0.;final_wall=[];last_day_start=d["expected_intervals"]-144
    with (root/"eplusout.csv").open(newline="") as f:
        reader=csv.DictReader(f);names=reader.fieldnames
        assert names and len(names)==len(set(names)) and names[0]=="Date/Time" and len(names)<=128
        for name in names[1:]:results[name]={"sum":0.,"correction":0.,"min":math.inf,"max":-math.inf}
        count=0
        for count,row in enumerate(reader,1):
            assert count<=d["expected_intervals"] and None not in row and all(x is not None for x in row.values())
            day,slot=divmod(count-1,144);date=dt.date(2025,1,1)+dt.timedelta(days=day);minute=(slot+1)*10
            assert row["Date/Time"].strip()==f"{date:%m/%d}  {minute//60:02}:{minute%60:02}:00"
            checks["timestamps"]+=1;values={}
            for name in names[1:]:
                value=float(row[name]);assert math.isfinite(value)
                if " [J]" in name or " [m3/s]" in name:assert value>=0
                checks["finite_values"]+=1;values[name]=value;s=results[name]
                total=s["sum"]+value
                s["correction"]+=(s["sum"]-total)+value if abs(s["sum"])>=abs(value) else (value-total)+s["sum"]
                s["sum"]=total;s["min"]=min(s["min"],value);s["max"]=max(s["max"],value)
            if d["case"]!="envelope":
                facility=values["Electricity:Facility [J](TimeStep)"]
                parts=[v for k,v in values.items() if k.endswith(":Electricity [J](TimeStep)") and not k.startswith("Electricity:Facility")]
                residual=abs(math.fsum(parts)-facility);maximum_residual_j=max(maximum_residual_j,residual)
                assert residual/600/1000<=max(.001,facility/600/1000*.0001);checks["meter_partition"]+=1
            if d["case"]=="electrical":
                assert values["Electricity:Facility [J](TimeStep)"]==d["oracle"]["every_interval_facility_j"]
            if d["case"]=="envelope" and count>last_day_start:
                final_wall.append(values["Z1S:Surface Inside Face Conduction Heat Transfer Rate [W](TimeStep)"])
                assert abs(values["Z1:Zone Mean Air Temperature [C](TimeStep)"]-25)<1e-6
            if d["case"] in ("dx","resource8"):
                supply=math.fsum(v for k,v in values.items() if k.endswith(":Fan Electricity Energy [J](TimeStep)"))
                vent=math.fsum(v for k,v in values.items() if ":Zone Ventilation Fan Electricity Energy " in k)
                assert abs(supply+vent-values["Fans:Electricity [J](TimeStep)"])<1e-6;checks["supply_ventilation_partition"]+=1
                assert abs(vent-d["zones"]*2400)<1e-6
                if d["case"]=="dx":
                    coil=values["Z1 COIL:Cooling Coil Electricity Energy [J](TimeStep)"]
                    assert abs(coil-values["Cooling:Electricity [J](TimeStep)"])<1e-6
                    assert abs(coil+supply-values["Z1 AC:Zone Window Air Conditioner Electricity Energy [J](TimeStep)"])<1e-6;checks["package_aggregate"]+=1
                    assert abs(values["Z1:Zone Ventilation Standard Density Volume Flow Rate [m3/s](TimeStep)"]-.02)<1e-12
                    if minute<=360:assert coil==0 and supply==0;checks["off_cycle"]+=1
    assert count==d["expected_intervals"]
    for name,s in results.items():
        s["sum"]+=s.pop("correction")
        if " [J]" in name:s["kwh"]=s["sum"]/3600000
    physics={}
    if d["case"]=="electrical":
        for name,target in [("Electricity:Facility",7008),("InteriorEquipment:Electricity",4380),("InteriorLights:Electricity",2190),("ExteriorLights:Electricity",438)]:
            actual=results[name+" [J](TimeStep)"]["kwh"];assert abs(actual-target)<=max(.01,target*.0001)
        physics={"fixed_electrical_oracle_pass":True}
    if d["case"]=="envelope":
        mean=math.fsum(final_wall)/len(final_wall);span=max(final_wall)-min(final_wall);difference=abs(mean-120/2.14)
        assert difference<=1 and span<=.01
        physics={"fixed_envelope_oracle_pass":True,"final_day_wall_w":mean,"independent_wall_w":120/2.14,"absolute_difference_w":difference,"final_day_range_w":span,"radiation_omission_bound_w":.001}
    files={p.name:{"bytes":p.stat().st_size,"sha256":sha(p)} for p in sorted(root.iterdir()) if p.is_file()}
    warnings=[x.strip() for x in error.splitlines() if "** Warning **" in x]
    return {"case":d["case"],"zones":d["zones"],"intervals":count,"channels":len(names)-1,"all_checks_pass":True,"checks":checks,"maximum_facility_residual_j":maximum_residual_j,"limited_physics":physics,"columns":results,"process":process,"files":files,"total_retained_bytes":sum(x["bytes"] for x in files.values()),"maximum_file_bytes":max(x["bytes"] for x in files.values()),"warnings":warnings,"qualification":False}


def production_integrity(runroot, item):
    fixed={"A10":("A",10),"B10":("B",10),"A5":("A",5),"B5":("B",5)}
    alternative,minutes=fixed[item["case"]]
    assert type(item["minutes"]) is int and (item["alternative"],item["minutes"])==(alternative,minutes)
    assert item["project"]==f"project-{minutes}" and runroot.is_dir() and not runroot.is_symlink()
    checksum_path=runroot/"checksums.json";checksum_hash=sha(checksum_path)
    seals=json.loads(checksum_path.read_text())["sha256"]
    required={"results.json","diagnostics.json","engine/process.json","engine/process-stdout.txt","engine/process-stderr.txt","engine/eplusout.err","engine/eplusout.end","model.epJSON","recipe.json","series/series.json"}
    assert required<=set(seals)
    for name,digest in seals.items():
        p=PurePosixPath(name)
        assert not p.is_absolute() and p.as_posix()==name and not any(x in (".","..") or ":" in x or "\\" in x for x in p.parts)
        path=runroot.joinpath(*p.parts)
        assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(runroot.resolve())
        assert sha(path)==digest
    actual={p.relative_to(runroot).as_posix() for p in runroot.rglob("*") if p.is_file()}
    assert actual==set(seals)|{"checksums.json"}
    record=json.loads((runroot/"results.json").read_text())
    process=json.loads((runroot/"engine/process.json").read_text())
    assert process["state"]=="completed" and type(process["exit_code"]) is int and process["exit_code"]==0
    assert process["process_tree_empty"] is True and process["failure"] is None
    assert process["arguments"]==["--jobs","1","--weather","weather.epw","--output-directory",".","--output-prefix","eplus","../model.epJSON"]
    assert process["executable"]=="energyplus.exe" and process["executable_sha256"]==record["method"]["engine_executable_sha256"]
    assert process["stdout_sha256"]==seals["engine/process-stdout.txt"] and process["stderr_sha256"]==seals["engine/process-stderr.txt"]
    for key,maximum in (("elapsed_ms",1800000),("sampled_peak_runner_plus_job_working_set_bytes",2147483648),("peak_observed_scratch_bytes",1073741824)):
        assert type(process[key]) is int and 0<process[key]<=maximum
    assert record["study"]["zone_timesteps_per_hour"]==60//minutes
    assert json.loads((runroot/"series/series.json").read_text())==record["series"]
    assert "EnergyPlus Completed Successfully" in (runroot/"engine/eplusout.end").read_text()
    errors=(runroot/"engine/eplusout.err").read_text()
    assert "** Severe" not in errors and "**  Fatal" not in errors
    return record,process,seals,checksum_hash


def production_case(root, item, jobs):
    """Independent raw-CSV/retained-f64 arithmetic, with no engine execution."""
    runroot=root/item["project"]/"runs"/item["id"]
    record,process,seals,checksum_hash=production_integrity(runroot,item)
    series=record["series"];channels=series["channels"];names=[c["id"] for c in channels]
    assert len(names)==len(set(names))<=64 and record["id"]==item["id"] and record["alternative_id"]==item["alternative"]
    minutes=item["minutes"];expected=525600//minutes
    assert series["interval_count"]==expected and series["interval_minutes"]==minutes
    assert series["encoding"]=="float64_little_endian" and series["ordering"]=="time_channel"
    diagnostic=json.loads((runroot/"diagnostics.json").read_text(encoding="utf-8-sig"))
    csvfile=jobs/item["id"]/"engine/eplusout.csv"
    assert sha(csvfile)==diagnostic["native_csv_sha256"] and csvfile.stat().st_size==diagnostic["native_csv_bytes"]<=134217728
    native=[c["label"] for c in channels if c["unit"]!="fraction"]
    month_energy=[{k:[] for k in ("facility_j","cooling_system","interior_lighting","exterior_lighting","plug_equipment","supply_fans","ventilation_exhaust_fans")} for _ in range(12)]
    aggregate={k:[] for k in month_energy[0]}
    comfort={z["id"]:[0,0] for z in record["model"]["zones"] if z["cooling_system"] is not None}
    schedules={s["id"]:s for s in record["model"]["schedules"]};weekday=["monday","tuesday","wednesday","thursday","friday","saturday","sunday"].index(record["study"]["calendar"]["start_weekday"])
    def schedule(id,index):
        ten=index*minutes//10;day,slot=divmod(ten,144);date=dt.date(2025,1,1)+dt.timedelta(days=day);s=schedules[id]
        holiday=next((h for h in s["holidays"] if h["date"]=={"month":date.month,"day":date.day}),None)
        kind=(["weekday"]*5+["saturday","sunday"])[(weekday+day)%7]
        profile=holiday["profile"] if holiday else s[kind]
        return profile["values"][slot]
    raw_comparisons=0;index=0;max_native_delta=0.;max_residual_kw=0.;building=[0,0];chunk_seals=[]
    with csvfile.open(newline="") as f:
        reader=csv.DictReader(f)
        assert reader.fieldnames and len(reader.fieldnames)==len(set(reader.fieldnames)) and set(reader.fieldnames)==set(native)|{"Date/Time"}
        for ci,chunk in enumerate(series["chunks"]):
            assert chunk["file"]==f"data-{ci:05}.f64" and chunk["start"]==index
            path=runroot/"series"/chunk["file"];data=path.read_bytes()
            assert len(data)==chunk["bytes"]==chunk["intervals"]*len(names)*8 and sha(path)==chunk["sha256"]
            chunk_seals.append(chunk)
            for tup in struct.iter_unpack("<"+"d"*len(names),data):
                assert index<expected and all(math.isfinite(v) for v in tup)
                row=next(reader);v=dict(zip(names,tup));day,slot=divmod(index,1440//minutes);date=dt.date(2025,1,1)+dt.timedelta(days=day);end=(slot+1)*minutes
                assert row["Date/Time"].strip()==f"{date:%m/%d}  {end//60:02}:{end%60:02}:00"
                for c,x in zip(channels,tup):
                    if c["unit"]=="J":assert x>=0
                    if c["unit"]!="fraction":
                        y=float(row[c["label"]]);assert math.isfinite(y) and x==y
                        max_native_delta=max(max_native_delta,abs(x-y));raw_comparisons+=1
                parts={"cooling_system":v["cooling_j"],"interior_lighting":v["interior_lighting_j"],"exterior_lighting":v["exterior_lighting_j"],"plug_equipment":v["equipment_j"],
                       "supply_fans":math.fsum(x for k,x in v.items() if k.endswith("_supply_fan_j")),"ventilation_exhaust_fans":math.fsum(x for k,x in v.items() if k.endswith("_ventilation_fan_j"))}
                residual=abs(math.fsum(parts.values())-v["facility_j"])/(minutes*60000)
                max_residual_kw=max(max_residual_kw,residual);assert residual<=max(.001,v["facility_j"]/(minutes*60000)*.0001)
                assert abs(parts["supply_fans"]+parts["ventilation_exhaust_fans"]-v["fans_j"])/(minutes*60000)<=max(.001,v["fans_j"]/(minutes*60000)*.0001)
                parts["facility_j"]=v["facility_j"]
                for k,x in parts.items():month_energy[date.month-1][k].append(x);aggregate[k].append(x)
                occ_any=False;unmet_any=False
                for zi,z in enumerate(record["model"]["zones"],1):
                    prefix=f"zone_{zi:03}";occ=schedule(z["gains"]["occupancy_schedule"],index)
                    assert v[prefix+"_occupancy"]==occ
                    if z["cooling_system"] is None:continue
                    target=schedule(z["cooling_system"]["cooling_setpoint_schedule"],index)
                    assert abs(v[prefix+"_setpoint_c"]-target)<=1e-7
                    unmet=occ>0 and v[prefix+"_temperature_c"]>target+.2
                    comfort[z["id"]][0]+=int(occ>0);comfort[z["id"]][1]+=int(unmet)
                    occ_any|=occ>0;unmet_any|=unmet
                building[0]+=int(occ_any);building[1]+=int(unmet_any);index+=1
        assert next(reader,None) is None and index==expected
    assert sha(csvfile)==diagnostic["native_csv_sha256"]
    checks=[]
    def close(id,actual,reference,absolute=1e-9,relative=1e-12):
        band=max(absolute,abs(reference)*relative);delta=abs(actual-reference)
        checks.append(dict(id=id,actual=actual,reference=reference,absolute_error=delta,band=band,passed=delta<=band))
    def energy(values):return {k:math.fsum(v)/3600000 for k,v in values.items()}
    annual=energy(aggregate)
    for label,computed,saved in [("annual",annual,record["totals"]["annual"])]+[(f"month{i+1}",energy(v),record["totals"]["monthly"][i]) for i,v in enumerate(month_energy)]:
        close(label+"/facility",computed["facility_j"],saved["facility_kwh"])
        for k,x in saved["end_uses_kwh"].items():close(label+"/"+k,computed[k],x)
        assert abs(math.fsum(v for k,v in computed.items() if k!="facility_j")-computed["facility_j"])<=max(.01,computed["facility_j"]*.0001)
    factor=10//minutes;aligned={k:[math.fsum(v[i:i+factor])/600000 for i in range(0,len(v),factor)] for k,v in aggregate.items()}
    peakindex=max(range(52560),key=lambda i:aligned["facility_j"][i]);peak=aligned["facility_j"][peakindex]
    peakday,peakslot=divmod(peakindex,144);peakdate=dt.date(2025,1,1)+dt.timedelta(days=peakday)
    assert record["totals"]["coincident_peak"]["timestamp"]==dict(interval_index=peakindex,month=peakdate.month,day=peakdate.day,start_minute=peakslot*10,end_minute=(peakslot+1)*10)
    close("peak/facility",peak,record["totals"]["coincident_peak"]["facility_kw"])
    for k,x in record["totals"]["coincident_peak"]["end_use_contributions_kw"].items():close("peak/"+k,aligned[k][peakindex],x)
    comfort_rows=[]
    for z,(occupied,unmet) in comfort.items():
        actual={"zone_id":z,"occupied_hours":occupied*minutes/60,"cooling_unmet_hours":unmet*minutes/60,"unmet_percent":100*unmet/occupied if occupied else None}
        saved=next(x for x in record["totals"]["zone_comfort"] if x["zone_id"]==z)
        for k in ("occupied_hours","cooling_unmet_hours","unmet_percent"):
            if actual[k] is not None:close(z+"/"+k,actual[k],saved[k])
            else:assert saved[k] is None
        actual["criterion_passed"]=actual["unmet_percent"] is not None and actual["unmet_percent"]<=1;comfort_rows.append(actual)
    for k,x in zip(("occupied_hours","cooling_unmet_hours"),building):close("building/"+k,x*minutes/60,record["totals"]["building_comfort_union"][k])
    assert all(c["passed"] for c in checks)
    assert sha(runroot/"checksums.json")==checksum_hash
    for name,digest in seals.items():assert sha(runroot/name)==digest
    return dict(case=item["case"],id=record["id"],minutes=minutes,rows=index,channels=len(channels),native_scalar_comparisons=raw_comparisons,max_native_scalar_difference=max_native_delta,
                annual_kwh=annual,aligned_peak_kw=peak,aligned_peak_index=peakindex,comfort=comfort_rows,arithmetic_checks=checks,maximum_interval_reconciliation_kw=max_residual_kw,
                method=record["method"],native_process=process,closed_run_integrity_verified=True,checksums_sha256=checksum_hash,
                files={str(f.relative_to(runroot)).replace("\\","/"):{"sha256":sha(f),"bytes":f.stat().st_size} for f in runroot.rglob("*") if f.is_file()},
                diagnostics=diagnostic,native_csv_sha256=sha(csvfile),model=record["model"],study=record["study"])


def production(root,jobs):
    index=json.loads((root/"run-index.json").read_text(encoding="utf-8-sig"));assert [x["case"] for x in index]==["A10","B10","A5","B5"]
    cases=[production_case(root,item,jobs) for item in index];comparisons=[]
    revision2=None
    if (root/"declaration.json").exists():
        declaration=json.loads((root/"declaration.json").read_text(encoding="utf-8-sig"))
        if declaration.get("example_revision")==2:
            protocol=Path(__file__).with_name("energy-zero-cutout-protocol.json")
            assert sha(protocol)==declaration["protocol_sha256"]=="64e3d780fee5ae5441c5150d864c5f4638c238591ed3988772fa55ff35bc3f40"
            plan=json.loads(protocol.read_text());historical=Path(__file__).parent.parent/plan["historical_inputs"]["path"]
            assert sha(historical)==plan["historical_inputs"]["sha256"]
            snapshots=json.loads(historical.read_text());canonical=lambda v:hashlib.sha256(json.dumps(v,sort_keys=True,separators=(",",":"),allow_nan=False).encode()).hexdigest()
            for c,p in zip(cases,plan["cases"]):
                assert c["case"]==p["case"] and c["method"]==p["expected_method"]
                expected=json.loads(json.dumps(snapshots["cases"][p["alternative"]]))
                for z in expected["model"]["zones"]:assert z["cooling_system"]["cooling_deadband_k"]==.2;z["cooling_system"]["cooling_deadband_k"]=0.0
                expected["study"]["zone_timesteps_per_hour"]=60//p["minutes"]
                assert c["model"]==expected["model"] and c["study"]==expected["study"]
                assert canonical(c["model"])==p["expected_model_canonical_sha256"] and canonical(c["study"])==p["expected_study_canonical_sha256"]
                preflight=json.loads((root/("input-preflight-"+c["case"]+".json")).read_text(encoding="utf-8-sig"))
                assert preflight["case"]==c["case"] and preflight["only_declared_changes"] is True and preflight["protocol_sha256"]==sha(protocol) and preflight["historical_inputs_sha256"]==sha(historical)
            revision2=dict(example_revision=2,protocol_sha256=sha(protocol),historical_inputs_sha256=sha(historical),only_eight_declared_cutout_changes_verified=True,original_failure_unchanged=True)
    for alternative in ("A","B"):
        ten=next(c for c in cases if c["case"]==alternative+"10");five=next(c for c in cases if c["case"]==alternative+"5")
        assert ten["model"]==five["model"]
        s=dict(ten["study"]);s["zone_timesteps_per_hour"]=12;assert s==five["study"]
        for field,threshold in (("annual_kwh",.01),("aligned_peak_kw",.03)):
            a=ten[field]["facility_j"] if field=="annual_kwh" else ten[field];b=five[field]["facility_j"] if field=="annual_kwh" else five[field]
            delta=abs(a-b);ratio=delta/abs(b) if b else (0 if a==0 else math.inf)
            comparisons.append(dict(alternative=alternative,quantity=field,ten_minute=a,five_minute_reference=b,relative_difference=ratio,limit=threshold,passed=ratio<=threshold))
    for c in cases:c.pop("model");c.pop("study")
    timestep_passed=all(c["passed"] for c in comparisons)
    comfort_passed=all(z["criterion_passed"] for c in cases for z in c["comfort"])
    return dict(schema="greenlens.energy-production-audit.v1",auditor_sha256=sha(Path(__file__)),cases=cases,timestep_comparisons=comparisons,
                timestep_passed=timestep_passed,comfort_passed=comfort_passed,all_checks_pass=timestep_passed and comfort_passed,
                revision2_contract=revision2,direct_csv_retention_and_arithmetic_passed=True,independent_native_rerun_or_replay_asserted=False,full_energy_method_qualified=False)


def direct_parity(root,production_root,jobs):
    """Exact independent native CSV equality is stronger than the declared 0.1% band.

    Derived statistics are bound to the previously completed raw/series audit,
    rather than pretending that equal input alone demonstrates output parity.
    """
    declaration=json.loads((root/"declaration.json").read_text(encoding="utf-8-sig"))
    execution=json.loads((root/"execution.json").read_text(encoding="utf-8-sig"))
    assert declaration["schema"]=="greenlens.energy-direct-native-parity.v1" and execution["completed"] is True
    assert declaration["native_processes"]==2 and declaration["workers"]==1
    for name,digest in declaration["source_sha256"].items():
        path=Path(__file__).parent.parent/"dependencies"/name if name=="energyplus.json" else Path(__file__).parent/name
        assert sha(path)==digest
    prior=production_root/"production-independent.json";prior_data=json.loads(prior.read_text())
    assert prior_data["direct_csv_retention_and_arithmetic_passed"] is True
    index=json.loads((production_root/"run-index.json").read_text(encoding="utf-8-sig"))
    assert [c["case"] for c in declaration["cases"]]==["A10","B10"]
    cases=[]
    for c,item in zip(declaration["cases"],index[:2]):
        assert c["case"]==item["case"] and c["original_run_id"]==item["id"]
        original=production_root/item["project"]/"runs"/item["id"]
        record,process,seals,checksum_hash=production_integrity(original,item)
        assert checksum_hash==c["original_checksums_sha256"] and seals["engine/process.json"]==c["original_process_sha256"]
        case_root=root/c["case"]
        assert c["input_sha256"]=={"model.epJSON":seals["model.epJSON"],"engine/weather.epw":seals["weather/normalized.epw"]}
        for name,digest in c["input_sha256"].items():assert sha(case_root/name)==digest
        native=json.loads((case_root/"engine/processes.json").read_text(encoding="utf-8-sig"))
        assert native["completed"] is True and type(native["exit_code"]) is int and native["exit_code"]==0
        assert native["job_empty"] is True and native["stream_tasks_completed"] is True and native["terminated_tree_reason"] is None
        assert native["arguments"]==c["arguments"]==process["arguments"] and native["executable"]=="energyplus.exe"
        limits=declaration["limits"]
        for actual,limit in (("wall_seconds","seconds"),("sampled_runner_plus_engine_peak_bytes","memory_bytes"),("sampled_scratch_peak_bytes","scratch_bytes")):
            assert 0<native[actual]<=limits[limit]
        files={p.relative_to(case_root).as_posix():{"sha256":sha(p),"bytes":p.stat().st_size} for p in case_root.rglob("*") if p.is_file()}
        assert max(v["bytes"] for v in files.values())<=limits["file_bytes"] and sum(v["bytes"] for v in files.values())<=limits["scratch_bytes"]
        csvfile=case_root/"engine/eplusout.csv";oldcsv=jobs/item["id"]/"engine/eplusout.csv"
        original_audit=next(x for x in prior_data["cases"] if x["case"]==c["case"])
        assert original_audit["checksums_sha256"]==checksum_hash
        assert sha(csvfile)==sha(oldcsv)==original_audit["native_csv_sha256"]
        compared_bytes=0
        with csvfile.open("rb") as new,oldcsv.open("rb") as old:
            while data:=new.read(65536):
                assert data==old.read(len(data));compared_bytes+=len(data)
            assert old.read(1)==b""
        with csvfile.open(newline="") as f:
            reader=csv.reader(f);header=next(reader);rows=sum(1 for row in reader)
        assert rows==original_audit["rows"]==52560
        for name,v in files.items():assert sha(case_root/name)==v["sha256"]
        cases.append(dict(case=c["case"],original_run_id=item["id"],rows=rows,native_columns=len(header)-1,
            byte_comparisons=compared_bytes,maximum_numeric_difference=0,all_reported_energy_and_interval_demand_pass=True,
            original_checksums_sha256=checksum_hash,original_raw_audit_sha256=sha(prior),process=native,files=files))
    return dict(schema="greenlens.energy-direct-native-parity-audit.v1",auditor_sha256=sha(Path(__file__)),declaration=declaration,
        declaration_sha256=sha(root/"declaration.json"),cases=cases,all_checks_pass=True,
        scope="Two fresh independent supervised native executions; complete CSV bytes equal the adapter CSVs whose full retained-series arithmetic was independently audited. No recognized replay or physical qualification inferred.",
        full_energy_method_qualified=False,source_pair_timestep_passed=prior_data["timestep_passed"],historical_failed_evidence_unchanged=True)


def control_number(text):
    assert re.fullmatch(r"[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?",text.strip())
    value=float(text);assert math.isfinite(value) and (value!=0 or Decimal(text)==0)
    return value


def control_effect(ten,five):
    assert len(ten)==len(five)==1728
    delta=[a-b for a,b in zip(ten,five)]
    relative=abs(max(ten)-max(five))/abs(max(five)) if max(five) else None
    return dict(ten_peak_kw=max(ten),five_peak_kw=max(five),ten_peak_index=ten.index(max(ten)),five_peak_index=five.index(max(five)),
        relative_peak_difference=relative,signed_mean_kw=math.fsum(delta)/len(delta),max_abs_kw=max(map(abs,delta)),rms_kw=math.sqrt(math.fsum(x*x for x in delta)/len(delta)))


def control_diagnostic(root,production_root):
    load=lambda p:json.loads(p.read_text(encoding="utf-8-sig"))
    protocol=Path(__file__).with_name("energy-control-protocol.json")
    assert sha(protocol)==sha(root/"proposal.json")=="062737b8ff28874bd453095a19006edd0be2e427d37dfcd7e3efd8fb53bd2ec4"
    plan=load(protocol);declaration=load(root/"declaration.json");execution=load(root/"execution.json")
    assert declaration["proposal_sha256"]==sha(protocol) and execution["completed"] is True
    assert declaration["workers"]==1 and declaration["native_processes"]==4
    start=dt.datetime.fromisoformat(declaration["declared_utc"]);deadline=dt.datetime.fromisoformat(declaration["deadline_utc"])
    assert (deadline-start).total_seconds()==7200 and start<=dt.datetime.fromisoformat(execution["finished_utc"])<=deadline
    assert 0<execution["wall_seconds"]<=7200
    for name,digest in declaration["source_sha256"].items():
        path=Path(__file__).parent.parent/"dependencies"/name if name=="energyplus.json" else Path(__file__).parent/name
        assert sha(path)==digest
    index=load(production_root/"run-index.json");item=next(x for x in index if x["case"]=="B10")
    assert item["id"]==plan["original_run_id"]
    original=production_root/item["project"]/"runs"/item["id"]
    record,_,seals,checksum=production_integrity(original,item)
    assert checksum==plan["original_checksums_sha256"] and seals["model.epJSON"]==plan["original_model_sha256"]
    assert seals["weather/normalized.epw"]==plan["original_weather_sha256"]
    channels=load(original/"series/series.json")["channels"]
    native={c["label"] for c in channels if c["unit"]!="fraction"}
    schedules={s["id"]:s for s in record["model"]["schedules"]}
    first_day=(dt.date(2025,12,20)-dt.date(2025,1,1)).days
    def scheduled(id,row,minutes):
        day,slot=divmod(first_day*144+row*minutes//10,144);date=dt.date(2025,1,1)+dt.timedelta(days=day);s=schedules[id]
        holiday=next((h for h in s["holidays"] if h["date"]=={"month":date.month,"day":date.day}),None)
        kind=(["weekday"]*5+["saturday","sunday"])[day%7]
        return (holiday["profile"] if holiday else s[kind])["values"][slot]
    cases=[];previous=start
    allfiles={p.relative_to(root).as_posix():{"sha256":sha(p),"bytes":p.stat().st_size} for p in root.rglob("*") if p.is_file()}
    assert max(v["bytes"] for v in allfiles.values())<=plan["limits"]["file_bytes"] and sum(v["bytes"] for v in allfiles.values())<=plan["limits"]["scratch_bytes"]
    for case in plan["cases"]:
        folder=root/case["case"];fixture=load(folder/"fixture-declaration.json");launch=load(folder/"execution-declaration.json");process=load(folder/"processes.json")
        assert fixture["input_sha256"]==case["input_sha256"] and fixture["proposal_sha256"]==sha(protocol)
        for name,digest in case["input_sha256"].items():assert sha(folder/name)==digest
        assert launch["fixture_sha256"]==sha(folder/"fixture-declaration.json") and launch["execute"] is True
        assert launch["dependency_sha256"]==declaration["source_sha256"]["energyplus.json"]
        assert launch["source_sha256"]=={k:declaration["source_sha256"][k] for k in ("energy-gate0-run.ps1","energy-gate0-fixture.py","annual-paired-process.ps1","annual-paired-native-process.cs")}
        assert process["arguments"]==launch["arguments"]==case["arguments"] and process["executable"]=="energyplus.exe"
        assert process["completed"] is True and type(process["exit_code"]) is int and process["exit_code"]==0 and process["job_empty"] is True and process["stream_tasks_completed"] is True and process["terminated_tree_reason"] is None
        assert previous<=dt.datetime.fromisoformat(process["started_utc"])<=dt.datetime.fromisoformat(process["finished_utc"])<=deadline
        previous=dt.datetime.fromisoformat(process["finished_utc"])
        limits=fixture["native_limits"];assert launch["limits"]==limits and 0<limits["timeout_seconds"]<=1800
        for field in ("memory_bytes","scratch_bytes","file_bytes"):assert limits[field]==plan["limits"][field]
        assert all(process["limits"][k]==v for k,v in limits.items())
        for actual,limit in (("wall_seconds","timeout_seconds"),("sampled_runner_plus_engine_peak_bytes","memory_bytes"),("sampled_scratch_peak_bytes","scratch_bytes")):assert 0<=process[actual]<=limits[limit]
        err=(folder/"eplusout.err").read_text();end=(folder/"eplusout.end").read_text()
        assert "Completed Successfully" in end and "0 Severe Errors" in end and "** Severe" not in err and "**  Fatal" not in err
        minutes=60//case["zone_timesteps_per_hour"];values={c["id"]:[] for c in channels};comfort=[[0,0] for z in record["model"]["zones"]];offsets=[];max_residual=0.
        with (folder/"eplusout.csv").open(newline="") as f:
            reader=csv.DictReader(f);assert len(reader.fieldnames)==len(native)+1 and set(reader.fieldnames)==native|{"Date/Time"}
            for i,row in enumerate(reader):
                assert i<case["expected_intervals"] and None not in row and all(v is not None for v in row.values())
                day,slot=divmod(i,1440//minutes);date=dt.date(2025,12,20)+dt.timedelta(days=day);endminute=(slot+1)*minutes
                assert row["Date/Time"].strip()==f"{date:%m/%d}  {endminute//60:02}:{endminute%60:02}:00"
                v={c["id"]:control_number(row[c["label"]]) for c in channels if c["unit"]!="fraction"}
                assert all(v[c["id"]]>=0 for c in channels if c["unit"]=="J")
                residual=abs(math.fsum(v[k] for k in ("cooling_j","fans_j","interior_lighting_j","exterior_lighting_j","equipment_j"))-v["facility_j"])/(minutes*60000)
                assert residual<=max(.001,v["facility_j"]/(minutes*60000)*.0001);max_residual=max(residual,max_residual)
                assert abs(math.fsum(x for k,x in v.items() if k.endswith("_supply_fan_j") or k.endswith("_ventilation_fan_j"))-v["fans_j"])/(minutes*60000)<=max(.001,v["fans_j"]/(minutes*60000)*.0001)
                for zi,z in enumerate(record["model"]["zones"],1):
                    prefix=f"zone_{zi:03}";occ=scheduled(z["gains"]["occupancy_schedule"],i,minutes);target=scheduled(z["cooling_system"]["cooling_setpoint_schedule"],i,minutes)
                    v[prefix+"_occupancy"]=occ;assert abs(v[prefix+"_setpoint_c"]-target)<=1e-7
                    comfort[zi-1][0]+=int(occ>0);comfort[zi-1][1]+=int(occ>0 and v[prefix+"_temperature_c"]>target+.2)
                    offsets.append(v[prefix+"_operative_setpoint_c"]-target)
                for k,x in v.items():values[k].append(x)
        assert len(values["facility_j"])==case["expected_intervals"]
        factor=10//minutes
        aligned={c["id"]:[math.fsum(values[c["id"]][i:i+factor])/(600000 if c["unit"]=="J" else factor) for i in range(0,case["expected_intervals"],factor)] for c in channels}
        coincidence=[sum(abs(aligned[f"zone_{z:03}_operative_setpoint_c"][i]-aligned[f"zone_{z:03}_setpoint_c"][i])>1e-7 for z in range(1,5)) for i in range(1728)]
        comfort_rows=[dict(zone_id=z["id"],occupied_hours=n*minutes/60,unmet_hours=u*minutes/60,unmet_percent=100*u/n if n else None) for z,(n,u) in zip(record["model"]["zones"],comfort)]
        cases.append(dict(case=case["case"],rows=case["expected_intervals"],process=process,energy_kwh=math.fsum(values["facility_j"])/3600000,
            maximum_reconciliation_kw=max_residual,maximum_operative_offset_c=max(map(abs,offsets)),comfort=comfort_rows,
            aligned_columns=[dict(id=c["id"],unit="kW" if c["unit"]=="J" else c["unit"]) for c in channels],aligned_values=aligned,
            aligned_nonzero_cutout_zone_count=coincidence,diagnostics_end=end.strip(),csv_sha256=sha(folder/"eplusout.csv")))
    comparisons=[]
    for ten,five in (cases[:2],cases[2:]):
        energy_delta=ten["energy_kwh"]-five["energy_kwh"]
        comparisons.append(dict(control=ten["case"].removesuffix("6"),energy_signed_kwh=energy_delta,energy_relative_difference=abs(energy_delta)/abs(five["energy_kwh"]) if five["energy_kwh"] else None,
            facility=control_effect(ten["aligned_values"]["facility_j"],five["aligned_values"]["facility_j"]),cooling=control_effect(ten["aligned_values"]["cooling_j"],five["aligned_values"]["cooling_j"])))
    a,b=[c["facility"]["relative_peak_difference"] for c in comparisons]
    falsified=[max(c["maximum_operative_offset_c"] for c in cases[2:])>1e-7,a is None or a<=.03,b is None or b>.03 or a is None or b>=a]
    for name,value in allfiles.items():assert sha(root/name)==value["sha256"]
    return dict(schema="greenlens.energy-fixed-control-audit.v1",auditor_sha256=sha(Path(__file__)),proposal_sha256=sha(protocol),declaration_sha256=sha(root/"declaration.json"),
        cases=cases,comparisons=comparisons,falsifiers=[dict(description=text,falsified=x) for text,x in zip(plan["falsifiers"],falsified)],
        all_declared_falsifiers_absent=not any(falsified),all_checks_pass=not any(falsified),files=allfiles,qualification_asserted=False,annual_acceptance_asserted=False,
        scope="Exactly four short fixed cases; distinct zero-cutout design and independent warmup. Original annual peak failure remains failed.")


if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--case-root",action="append",type=Path);p.add_argument("--production-root",type=Path);p.add_argument("--direct-root",type=Path);p.add_argument("--control-root",type=Path);p.add_argument("--jobs-root",type=Path);p.add_argument("--output",type=Path,required=True);a=p.parse_args()
    assert not a.output.exists()
    if a.control_root:
        assert a.production_root and not a.direct_root and not a.jobs_root and not a.case_root
        result=control_diagnostic(a.control_root,a.production_root)
    elif a.direct_root:
        assert a.production_root and a.jobs_root and not a.case_root
        result=direct_parity(a.direct_root,a.production_root,a.jobs_root)
    elif a.production_root:
        assert a.jobs_root and not a.case_root
        result=production(a.production_root,a.jobs_root)
    else:
        assert a.case_root and not a.jobs_root
        result={"schema":"greenlens.energy-gate0-audit.v1","auditor_sha256":sha(Path(__file__)),"cases":[audit(x) for x in a.case_root],"full_energy_method_qualified":False,"real_weather_qualified":False,"demo_timestep_qualified":False,"resource_scope":"original massless synthetic envelope and constant zero-solar weather; not worst-case every supported input"}
    a.output.write_text(json.dumps(result,indent=2,allow_nan=False)+"\n",encoding="utf8")
    print(json.dumps({"sha256":sha(a.output),"bytes":a.output.stat().st_size,"cases":[r["case"] for r in result["cases"]],"timestep_passed":result.get("timestep_passed"),"comfort_passed":result.get("comfort_passed")}))
