"""Pure closed-record guard regressions; no native engine or scientific result."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location("energy_audit",Path(__file__).with_name("energy-gate0-audit.py"))
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


class ClosedRecord(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        self.item=dict(case="A10",alternative="A",minutes=10,project="project-10",id="synthetic")
        self.documents={
            "results.json":dict(method=dict(engine_executable_sha256="a"*64),study=dict(zone_timesteps_per_hour=6),series={}),
            "diagnostics.json":{},"series/series.json":{},"model.epJSON":{},"recipe.json":{},
            "engine/process.json":dict(state="completed",exit_code=0,process_tree_empty=True,failure=None,
                executable="energyplus.exe",executable_sha256="a"*64,
                arguments=["--jobs","1","--weather","weather.epw","--output-directory",".","--output-prefix","eplus","../model.epJSON"],
                elapsed_ms=100,sampled_peak_runner_plus_job_working_set_bytes=1000,peak_observed_scratch_bytes=1000)}
        for name,value in {"engine/process-stdout.txt":"output","engine/process-stderr.txt":"","engine/eplusout.err":"","engine/eplusout.end":"EnergyPlus Completed Successfully"}.items():
            path=self.root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(value)
        p=self.documents["engine/process.json"]
        p["stdout_sha256"]=A.sha(self.root/"engine/process-stdout.txt");p["stderr_sha256"]=A.sha(self.root/"engine/process-stderr.txt")
        self.seal()

    def seal(self):
        for name,value in self.documents.items():
            path=self.root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(value))
        seals={p.relative_to(self.root).as_posix():A.sha(p) for p in self.root.rglob("*") if p.is_file() and p.name!="checksums.json"}
        (self.root/"checksums.json").write_text(json.dumps(dict(sha256=seals)))

    def reject(self):
        with self.assertRaises((AssertionError,KeyError,FileNotFoundError)):A.production_integrity(self.root,self.item)

    def test_positive(self):A.production_integrity(self.root,self.item)
    def test_failed_or_partial_process(self):
        original=copy.deepcopy(self.documents["engine/process.json"])
        for key,value in (("exit_code",1),("exit_code",False),("state","running"),("process_tree_empty",False),("failure","partial")):
            self.documents["engine/process.json"]=dict(original,**{key:value});self.seal();self.reject()
    def test_changed_results_or_diagnostics(self):
        for name in ("results.json","diagnostics.json"):
            path=self.root/name;old=path.read_bytes();path.write_bytes(old+b" ");self.reject();path.write_bytes(old)
    def test_extra_or_missing_member(self):
        extra=self.root/"extra.txt";extra.write_text("unexpected");self.reject();extra.unlink()
        (self.root/"diagnostics.json").unlink();self.reject()
    def test_case_minutes(self):
        for value in (5,True):self.item["minutes"]=value;self.reject()
    def test_coherent_wrong_timestep(self):
        self.documents["results.json"]["study"]["zone_timesteps_per_hour"]=12;self.seal();self.reject()
    def test_arguments_engine_or_resource(self):
        original=copy.deepcopy(self.documents["engine/process.json"])
        for key,value in (("arguments",["injected"]),("executable_sha256","b"*64),("elapsed_ms",1800001),("sampled_peak_runner_plus_job_working_set_bytes",2147483649),("peak_observed_scratch_bytes",1073741825)):
            self.documents["engine/process.json"]=dict(original,**{key:value});self.seal();self.reject()
    def test_unsafe_member(self):
        path=self.root/"checksums.json";v=json.loads(path.read_text());v["sha256"]["../outside"]="a"*64;path.write_text(json.dumps(v));self.reject()
    def test_production_gate_preserves_failed_timestep_or_comfort(self):
        (self.root/"run-index.json").write_text(json.dumps([dict(case=c) for c in ("A10","B10","A5","B5")]))
        for timestep_ok,comfort_ok in ((True,True),(False,True),(True,False),(False,False)):
            def case(root,item,jobs):
                c=item["case"];five=c.endswith("5")
                return dict(case=c,model={},study=dict(zone_timesteps_per_hour=12 if five else 6),
                    annual_kwh=dict(facility_j=100),aligned_peak_kw=10 if five or timestep_ok else 11,
                    comfort=[dict(criterion_passed=comfort_ok)])
            with patch.object(A,"production_case",side_effect=case):result=A.production(self.root,self.root)
            self.assertEqual(result["all_checks_pass"],timestep_ok and comfort_ok)
            self.assertEqual(len(result["cases"]),4);self.assertEqual(len(result["timestep_comparisons"]),4)
            self.assertFalse(result["full_energy_method_qualified"])

    def test_revision2_input_and_method_contract(self):
        plan=json.loads(Path(__file__).with_name("energy-zero-cutout-protocol.json").read_text())
        snapshots=json.loads((Path(__file__).parent.parent/plan["historical_inputs"]["path"]).read_text())
        protocol_hash=A.sha(Path(__file__).with_name("energy-zero-cutout-protocol.json"))
        (self.root/"declaration.json").write_text(json.dumps(dict(example_revision=2,protocol_sha256=protocol_hash)))
        (self.root/"run-index.json").write_text(json.dumps([dict(case=c) for c in plan["order"]]))
        for c in plan["cases"]:
            (self.root/("input-preflight-"+c["case"]+".json")).write_text(json.dumps(dict(case=c["case"],only_declared_changes=True,protocol_sha256=protocol_hash,historical_inputs_sha256=plan["historical_inputs"]["sha256"])))
        for mutation in (None,"capacity","method","cutout","calendar"):
            def case(root,item,jobs):
                p=next(c for c in plan["cases"] if c["case"]==item["case"])
                value=copy.deepcopy(snapshots["cases"][p["alternative"]]);value["study"]["zone_timesteps_per_hour"]=60//p["minutes"]
                for z in value["model"]["zones"]:z["cooling_system"]["cooling_deadband_k"]=0.0
                value.update(case=item["case"],method=copy.deepcopy(p["expected_method"]),annual_kwh=dict(facility_j=100),aligned_peak_kw=10,comfort=[dict(criterion_passed=True)])
                if mutation=="capacity":value["model"]["zones"][0]["cooling_system"]["gross_rated_capacity_w"]+=1
                if mutation=="method":value["method"]["recipe_sha256"]="0"*64
                if mutation=="cutout":value["model"]["zones"][0]["cooling_system"]["cooling_deadband_k"]=.2
                if mutation=="calendar":value["study"]["calendar"]["start_weekday"]="tuesday"
                return value
            with patch.object(A,"production_case",side_effect=case):
                if mutation:
                    with self.assertRaises(AssertionError):A.production(self.root,self.root)
                else:self.assertTrue(A.production(self.root,self.root)["revision2_contract"]["only_eight_declared_cutout_changes_verified"])


class FixedControl(unittest.TestCase):
    def test_numeric_contract(self):
        for text in ("nan","Infinity","1e-999","1e999","1_000","0x1","True",""):
            with self.assertRaises((AssertionError,ValueError)):A.control_number(text)
        for text,expected in (("0e-999",0),("-0.0",0),("1.25e-2",.0125),("-2",-2)):
            self.assertEqual(A.control_number(text),expected)

    def test_fixed_common_bins(self):
        a=[2.0]*1728;b=[1.0]*1728;a[27]=4;b[33]=3
        value=A.control_effect(a,b)
        self.assertEqual(value["ten_peak_index"],27);self.assertEqual(value["five_peak_index"],33)
        self.assertEqual(value["relative_peak_difference"],1/3)
        self.assertEqual(value["max_abs_kw"],3)
        self.assertIsNone(A.control_effect([0.0]*1728,[0.0]*1728)["relative_peak_difference"])
        with self.assertRaises(AssertionError):A.control_effect(a[:-1],b)


if __name__=="__main__":unittest.main()
