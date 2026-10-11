#!/usr/bin/env python3
"""Reproduce the retained provisional A s000023 DA failure without engine execution.

Standard library only, Python 3.10+. This deliberately bounded inspector expects
four 24-sensor, 8760-hour exports plus convergence.json from the recorded failed
candidate. It is not GreenLens's general-purpose archive importer. Original
arithmetic is retained from the independently recorded local inspection; paths
and output are explicit arguments. Every computed report field is checked
against the preserved diagnostic before a new output file is created.
"""
import argparse,json,zipfile,hashlib,struct,math,re,statistics,datetime
from pathlib import Path

if not __debug__:
    raise SystemExit("Run without -O: integrity assertions must remain enabled.")
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--convergence-dir',type=Path,required=True,
                    help='Directory containing convergence.json and shipping/reference-A/B.zip')
parser.add_argument('--output',type=Path,required=True,
                    help='New JSON output path; existing files are never overwritten')
parser.add_argument('--check-record',type=Path,
                    default=Path(__file__).resolve().parents[1]/'docs/evidence/v0.3-integration/annual-da-diagnostic.json',
                    help='Preserved diagnostic JSON to verify (defaults to the repository record)')
args=parser.parse_args()
convergence_dir=args.convergence_dir.resolve(strict=True)
output=args.output.resolve()
if not convergence_dir.is_dir():
    parser.error('--convergence-dir must be a directory')
if output.exists():
    parser.error('--output already exists; choose a new file')
if not output.parent.is_dir():
    parser.error('--output parent directory must already exist')
H=lambda b:hashlib.sha256(b).hexdigest()
J=lambda b:json.loads(b.decode('utf-8-sig'))
records={};arrays={};weather={};suns={};origins={};validation=[]
convergence=J((convergence_dir/'convergence.json').read_bytes())
for role in ['shipping','reference']:
 for alternative in ['A','B']:
  key=f'{role}-{alternative}';archive=convergence_dir/f'{key}.zip'
  with zipfile.ZipFile(archive) as z:
   seal=J(z.read('checksums.json'))['sha256']
   assert len(z.namelist())==len(set(z.namelist()))
   assert set(z.namelist())==set(seal)|{'checksums.json'}
   for name,digest in seal.items(): assert H(z.read(name))==digest,(key,name)
   project=J(z.read('project.json'));summary=project['climate']['runs'][0];prefix=summary['path']+'/'
   rec=J(z.read(prefix+'results.json')); records[key]=rec
   for name,digest in J(z.read(prefix+'checksums.json'))['sha256'].items():assert H(z.read(prefix+name))==digest
   axis=J(z.read(prefix+'weather/weather.json')); weather[key]=axis
   rows=[line.split(',') for line in z.read(prefix+'annual-weather.epw').decode().splitlines()[8:]]
   assert len(rows)==8760
   origins[key]=rows
   m=rec['series'];assert m['channels']==['R','G','B','illuminance'] and m['ordering']=='time_sensor_channel'
   count=len(m['sensor_ids']);hours=[];max_rgb_delta=0
   for c in m['chunks']:
    b=z.read(prefix+'series/'+c['file']);assert len(b)==c['bytes'] and H(b)==c['sha256'] and c['start']==len(hours)
    vals=list(struct.iter_unpack('<dddd',b)); assert len(vals)==c['intervals']*count
    assert all(math.isfinite(v) and v>=0 for tup in vals for v in tup)
    for r,g,blue,lux in vals:max_rgb_delta=max(max_rgb_delta,abs(179*(.265074126*r+.670114631*g+.064811243*blue)-lux))
    hours.extend([[vals[h*count+s][3] for s in range(count)] for h in range(c['intervals'])])
   assert len(hours)==8760 and count==24;arrays[key]=hours
   solar={}
   for match in re.finditer(r'void light solar(\d+)\s+0\s+0\s+3\s+(.*?)\s+solar\d+ source \S+\s+0\s+0\s+4\s+(.*?)(?=\n\s*void light|\Z)',z.read(prefix+'annual-suns.rad').decode(),re.S):
    index=(int(match[1])-30)//60;rgb=list(map(float,match[2].split()));direction=list(map(float,match[3].split()))
    assert len(rgb)==3 and len(direction)==4
    norm=math.sqrt(sum(v*v for v in direction[:3]));solar[index]={'radiance_rgb':rgb,'direction':[v/norm for v in direction[:3]],'diameter':direction[3]}
   assert solar,'solar source parse failed';suns[key]=solar
   occupied=[i for i in range(8760) if 8<=i%24<18]
   settings=rec['study']['metrics'];assert settings=={'da_threshold_lux':300.0,'udi_lower_lux':100.0,'udi_upper_lux':3000.0,'occupancy':{'kind':'daily','start_minute':480,'end_minute':1080}}
   max_metric_delta=0;recomputed=[]
   for s in range(count):
    v=[hours[i][s] for i in occupied];n=len(v)
    metric={'da_percent':sum(x>=300 for x in v)/n*100,'cda_percent':math.fsum(min(x/300,1) for x in v)/n*100,'udi_percent':sum(100<=x<=3000 for x in v)/n*100,'underlit_percent':sum(x<100 for x in v)/n*100,'overlit_percent':sum(x>3000 for x in v)/n*100}
    max_metric_delta=max(max_metric_delta,max(abs(metric[k]-rec['metrics'][s][k]) for k in metric));recomputed.append(metric)
   assert max_metric_delta<1e-10 and max_rgb_delta==0
   validation.append({'slot':key,'archive_sha256':H(archive.read_bytes()),'run_id':rec['id'],'results_sha256':seal[prefix+'results.json'],'record_sha256':summary['record_sha256'],'sealed_members_verified':len(seal),'intervals':len(hours),'sensors':count,'source_commit':rec['provenance']['source_commit'],'source_worktree_dirty':rec['provenance']['source_worktree_dirty'],'profile':rec['provenance']['profile'],'engine_fingerprint':rec['provenance']['engine']['fingerprint'],'max_rgb_conversion_difference_lux':max_rgb_delta,'max_recomputed_metric_difference_pp':max_metric_delta,'target_metrics':recomputed[22]})
# Four immutable sets must have identical weather, sensors and metrics. Profiles alone differ within A or B.
base=records['shipping-A']
for key,r in records.items():
 assert r['study']['calculation']['weather']==base['study']['calculation']['weather']
 assert r['study']['metrics']==base['study']['metrics']
 assert r['validated']['sensor_grid']==base['validated']['sensor_grid']
 assert r['provenance']['engine']==base['provenance']['engine']
 assert origins[key]==origins['shipping-A']
for a in ['A','B']:assert records['shipping-'+a]['model']==records['reference-'+a]['model']
for role in ['shipping','reference']:
 ma=json.loads(json.dumps(records[role+'-A']['model']));mb=json.loads(json.dumps(records[role+'-B']['model']));ma.pop('name');mb.pop('name');ma.pop('overhang');mb.pop('overhang');assert ma==mb
occupied=[i for i in range(8760) if 8<=i%24<18]
sensor=base['validated']['sensor_grid']['sensors'][22]
assert sensor['id']=='s000023'
# The preserved independent direction bound below is specific to this room.
# Reject changed geometry instead of silently applying hard-coded aperture bounds.
assert base['model']['room']=={'width_m':4.0,'depth_m':6.0,'height_m':3.0,'rotation_deg':0.0}
assert base['model']['window']=={'width_m':1.8,'height_m':1.5,'sill_m':0.9,'center_x_m':2.0}
assert base['model']['overhang'] is None and base['model']['obstruction'] is None
assert sensor['position']==[2.4375,5.291666666666666,0.8]
def stats(v):return {'count':len(v),'min':min(v) if v else None,'mean':statistics.fmean(v) if v else None,'median':statistics.median(v) if v else None,'max':max(v) if v else None}
def condition(i):
 row=origins['shipping-A'][i];dni=float(row[14]);dhi=float(row[15]);return 'low_diffuse' if dhi<50 else 'clear' if dni/dhi>=3 else 'mixed' if dni/dhi>=.5 else 'overcast'
def direct_bounds(i):
 src=suns['shipping-A'].get(i)
 if not src:return {'solar_source':False,'all_disc_rays_miss_aperture':True}
 d=src['direction'];p=sensor['position'];rho=src['diameter']*math.pi/360;eps=.0001
 # Bound all solar-cap direction components independently. A ray ratio's broad
 # interval enclosing rectangle deliberately favors possible intersection.
 def component(k):
  half=math.sqrt(max(0,1-d[k]**2))*math.sin(rho)
  return (-1 if math.acos(-d[k])<=rho else d[k]*math.cos(rho)-half,1 if math.acos(d[k])<=rho else d[k]*math.cos(rho)+half)
 dx=component(0);dy=component(1);dz=component(2)
 result={'solar_source':True,'center_altitude_deg':math.degrees(math.asin(d[2])),'center_direction':d}
 aperture_max_altitude=math.atan2(2.4-p[2]-eps,p[1]);result['aperture_max_altitude_deg']=math.degrees(aperture_max_altitude)
 if math.asin(d[2])-rho>aperture_max_altitude:result['all_disc_rays_miss_aperture']=True;result['exclusion']='whole solar disc above maximum aperture altitude';return result
 if dy[0]>=0:result['all_disc_rays_miss_aperture']=True;result['exclusion']='whole solar disc points away from facade';return result
 if dy[1]>=0:result['all_disc_rays_miss_aperture']=False;return result
 tlo=p[1]/(-dy[0]);thi=p[1]/(-dy[1]);xr=[p[0]+t*v for t in (tlo,thi) for v in dx];zr=[p[2]+eps+t*v for t in (tlo,thi) for v in dz]
 result.update({'aperture_x_bound_m':[min(xr),max(xr)],'aperture_z_bound_m':[min(zr),max(zr)],'all_disc_rays_miss_aperture':max(xr)<1.1 or min(xr)>2.9 or max(zr)<.9 or min(zr)>2.4})
 return result
cross=[];pair_summary={}
for a in ['A','B']:
 ship=arrays['shipping-'+a];ref=arrays['reference-'+a]
 intervals=[i for i in occupied if (ship[i][22]>=300)!=(ref[i][22]>=300)]
 pair_summary[a]={'shipping_above_count':sum(ship[i][22]>=300 for i in occupied),'reference_above_count':sum(ref[i][22]>=300 for i in occupied),'shipping_only_above_count':sum(ship[i][22]>=300 and ref[i][22]<300 for i in occupied),'reference_only_above_count':sum(ship[i][22]<300 and ref[i][22]>=300 for i in occupied),'occupied_lux_difference_shipping_minus_reference':stats([ship[i][22]-ref[i][22] for i in occupied]),'da_difference_pp':records['shipping-'+a]['metrics'][22]['da_percent']-records['reference-'+a]['metrics'][22]['da_percent'],'cda_difference_pp':records['shipping-'+a]['metrics'][22]['cda_percent']-records['reference-'+a]['metrics'][22]['cda_percent'],'crossing_intervals':intervals}
 if a=='A':
  for i in intervals:
   row=origins['shipping-A'][i];cross.append({'interval_index':i,'month':int(row[1]),'day':int(row[2]),'hour_ending':int(row[3]),'source_year_label':int(row[0]),'condition':condition(i),'dni_wh_m2':float(row[14]),'dhi_wh_m2':float(row[15]),'shipping_lux':ship[i][22],'reference_lux':ref[i][22],'difference_lux':ship[i][22]-ref[i][22],'shipping_above':ship[i][22]>=300,'reference_above':ref[i][22]>=300,'reference_distance_to_threshold_lux':ref[i][22]-300,'solar_geometry':direct_bounds(i)})
def grouped(rows,key):
 result=[]
 for group in sorted(set(r[key] for r in rows)):
  group_rows=[r for r in rows if r[key]==group]
  result.append({key:group,'count':len(group_rows),'shipping_only':sum(r['shipping_above'] for r in group_rows),'reference_only':sum(r['reference_above'] for r in group_rows),'mean_shipping_minus_reference_lux':statistics.fmean(r['difference_lux'] for r in group_rows),'cda_crossing_difference_pp':math.fsum(min(r['shipping_lux']/300,1)-min(r['reference_lux']/300,1) for r in group_rows)/3650*100})
 return result
A=arrays['shipping-A'];R=arrays['reference-A'];cross_ids={r['interval_index'] for r in cross}
remainder=[i for i in occupied if i not in cross_ids]
profile_deltas={k:{'shipping':base['provenance']['profile'].get(k),'reference':records['reference-A']['provenance']['profile'].get(k)} for k in base['provenance']['profile'] if base['provenance']['profile'][k]!=records['reference-A']['provenance']['profile'][k]}
report={'schema_version':'greenlens.annual-da-diagnostic.v0.3','generated_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Independent arithmetic/identity diagnostic of four completed retained annual series. No engine rerun, no independent physical accuracy claim, no changed threshold or exclusion. Prior convergence remains failed.','source_location':'Supplied --convergence-dir; absolute path intentionally omitted','diagnostic_method':'Verify every archive member SHA and nested run seal, decode little-endian time/sensor/RGB/lux records, verify RGB conversion, independently recompute all five metrics for all 24 sensors in four runs. Daily occupied intervals are 08:00 through 18:00 local standard time, HE9-18, including every day. DA includes 300 lx. Analyze every occupied threshold disagreement for s000023. No OS time conversions.','input_convergence_sha256':H((convergence_dir/'convergence.json').read_bytes()),'original_convergence_failure_details':convergence['failure_details'],'validated_sources':validation,'sensor':sensor,'weather_binding':base['study']['calculation']['weather'],'occupancy':base['study']['metrics']['occupancy'],'occupied_hour_count':len(occupied),'threshold_lux':300,'fixed_convergence_tolerance_pp':2,'profile_differences':profile_deltas,'pair_summary':pair_summary,'A_crossings_by_month':grouped(cross,'month'),'A_crossings_by_hour_ending':grouped(cross,'hour_ending'),'A_crossings_by_diagnostic_condition':grouped(cross,'condition'),'diagnostic_condition_policy':'low_diffuse if DHI<50 Wh/m2; otherwise clear if DNI/DHI>=3, mixed if>=0.5, else overcast. Descriptive radiation grouping, not an independent sky classification.','A_crossing_lux_difference':stats([r['difference_lux'] for r in cross]),'A_absolute_reference_distance_at_crossing_lux':stats([abs(r['reference_distance_to_threshold_lux']) for r in cross]),'A_crossing_cda_difference_pp':math.fsum(min(A[i][22]/300,1)-min(R[i][22]/300,1) for i in cross_ids)/3650*100,'A_non_crossing_cda_difference_pp':math.fsum(min(A[i][22]/300,1)-min(R[i][22]/300,1) for i in remainder)/3650*100,'A_crossing_all_disc_rays_provably_miss_aperture_count':sum(r['solar_geometry']['all_disc_rays_miss_aperture'] for r in cross),'direct_visibility_bound_method':'For each emitted solar cap, independent component min/max bounds enclose all ray directions. Since facade is y=0, interval arithmetic bounds the aperture x/z intersection. A bound fully outside the rectangle proves no direct ray; possible intersection is not proof of illumination. Uses effective sensor z+1e-4 m.','A_all_crossings':cross}
# Independent timing and conservation sanity checks, using every retained hour.
zero_indices=[i for i,row in enumerate(origins['shipping-A']) if float(row[14])==0 and float(row[15])==0]
zero_checks={key:{'intervals':len(zero_indices),'maximum_lux':max(max(values[i]) for i in zero_indices)} for key,values in arrays.items()}
assert all(check['maximum_lux']==0 for check in zero_checks.values())
lag_checks=[]
for lag in [-24,-2,-1,0,1,2,24]:
 pairs=[(A[i][22],R[i+lag][22]) for i in occupied if 0<=i+lag<8760]
 xx=[x for x,y in pairs];yy=[y for x,y in pairs]
 lag_checks.append({'reference_offset_hours':lag,'count':len(pairs),'mean_absolute_difference_lux':statistics.fmean(abs(x-y) for x,y in pairs),'pearson_correlation':statistics.correlation(xx,yy)})
assert suns['shipping-A']==suns['reference-A']==suns['shipping-B']==suns['reference-B']
report['generated_solar_sources_identical_all_four_runs']=True
report['generated_solar_source_count']=len(suns['shipping-A'])
report['zero_dni_and_dhi_checks']=zero_checks
report['same_sensor_lag_diagnostic']=lag_checks
report['near_threshold_crossing_counts']={str(distance):sum(abs(r['reference_distance_to_threshold_lux'])<=distance for r in cross) for distance in [1,2,5,10,20,50]}
report['A_all_occupied_hour_sign_counts']={'shipping_less':sum(A[i][22]<R[i][22] for i in occupied),'equal':sum(A[i][22]==R[i][22] for i in occupied),'shipping_greater':sum(A[i][22]>R[i][22] for i in occupied)}
report['A_cda_difference_components']=[{'classification':label,'hours':len(indices),'contribution_shipping_minus_reference_pp':math.fsum(min(A[i][22]/300,1)-min(R[i][22]/300,1) for i in indices)/3650*100} for label,indices in [('both_below',[i for i in occupied if A[i][22]<300 and R[i][22]<300]),('both_above',[i for i in occupied if A[i][22]>=300 and R[i][22]>=300]),('threshold_disagreement',sorted(cross_ids))]]
report['A_occupied_by_hour_ending']=[{'hour_ending':h+1,'count':365,'shipping_less':sum(A[i][22]<R[i][22] for i in occupied if i%24==h),'shipping_greater':sum(A[i][22]>R[i][22] for i in occupied if i%24==h),'shipping_mean_lux':statistics.fmean(A[i][22] for i in occupied if i%24==h),'reference_mean_lux':statistics.fmean(R[i][22] for i in occupied if i%24==h),'mean_difference_lux':statistics.fmean(A[i][22]-R[i][22] for i in occupied if i%24==h)} for h in range(8,18)]
report['interpretation']=[
 'All 480 stored metric values independently recompute exactly from retained lux samples, and all RGB-to-lux conversions agree exactly. Sensor IDs/order, time axis, weather and occupancy match across all four runs. No metric arithmetic/serialization error was found in these retained inputs.',
 'The 83 occupied threshold disagreements are all shipping below 300 lx and reference at or above 300 lx. This is a real failed convergence result, not a count or denominator mismatch. The unchanged 2 percentage-point limit permits 73 net hourly classification differences at this occupancy; observed 83 exceeds it by 10.',
 'DA is discontinuous at 300 lx; crossing hours contribute only about -0.04214 percentage points to cDA, while hours below threshold in both profiles contribute about -0.80645 points. This is consistent with broader small illuminance differences plus threshold crossings, not a DA/cDA implementation contradiction.',
 'Every A crossing solar disc misses the window by an independently conservative aperture-direction bound. A has no overhang or surrounding obstruction. Thus the crossing differences cannot be attributed to the 1024-versus-4096 direct finite-disc quadrature at this sensor; diffuse/ground transport and/or reflected-solar/profile differences remain candidates.',
 'The profiles jointly change sky basis MF2/MF4, bounce depth 8/9, full/direct coefficient sampling 131072/262144, and finite-disc samples. Retained combined arrays do not separate those components, so sampling alone cannot be established as the cause and the reference is not treated as ground truth.',
 'Same-hour series alignment has substantially lower error than the inspected +/-1, +/-2 and +/-24 hour shifts. This supports correct paired indexing but is not an independent validation of the solar/time model.',
 'No engine, matrix or light transport rerun was performed. All failed crossing samples and the original failed convergence check are retained; no tolerance, occupancy or sample was changed.'
]
report['model_difference']={'A_overhang':records['shipping-A']['model']['overhang'],'B_overhang':records['shipping-B']['model']['overhang'],'obstruction':base['model']['obstruction'],'window':base['model']['window'],'glazing_normal_transmittance':base['model']['materials']['visible_transmittance'],'shipping_reference_models_identical_per_alternative':True,'A_B_only_scientific_difference':'overhang; names are display metadata'}
report['direct_visibility_bound_method']+=' Also exclude a cap wholly above atan2(window_head - effective_sensor_z, facade_distance), the largest possible altitude to any point of the aperture; component ranges include +/- axis directions when inside the cap.'
# Recompute the compact publication summaries, preserving the original record.
report['diagnostic_script_sha256']=H(Path(__file__).read_bytes())
report['diagnostic_script_location']='scripts/inspect-annual-da.py'
report['diagnostic_runtime']='Python standard library used only for this independent retained-data analysis; not a GreenLens runtime requirement.'
report['conclusions_provisional']=True
report['release_acceptance_passed']=False
report['A_crossings_by_season']=[{'season':season,'months':months,'count':sum(row['month'] in months for row in cross)} for season,months in [('DJF',[12,1,2]),('MAM',[3,4,5]),('JJA',[6,7,8]),('SON',[9,10,11])]]
report['aperture_maximum_altitude_deg']=math.degrees(math.atan2(2.4-sensor['position'][2]-.0001,sensor['position'][1]))
# Match the original publication's compact geometry representation: the shared
# altitude limit is retained above, solar vectors remain in the sealed sources,
# and the four interval-bound exclusions have a readable reason.
for crossing in report['A_all_crossings']:
    geometry=crossing['solar_geometry']
    geometry.pop('center_direction',None)
    geometry.pop('aperture_max_altitude_deg',None)
    if geometry['all_disc_rays_miss_aperture'] and 'exclusion' not in geometry:
        geometry['exclusion']='conservative aperture x/z interval does not intersect window'
expected=J(args.check_record.read_bytes())
# These four provenance fields describe the original inspection or this repeat,
# not its mathematical results. Every other field, including failures, must match.
repeat_metadata={'generated_utc','source_location','diagnostic_script_sha256','diagnostic_script_location'}
actual_results={k:v for k,v in report.items() if k not in repeat_metadata}
expected_results={k:v for k,v in expected.items() if k not in repeat_metadata}
if actual_results != expected_results:
    mismatches=sorted(k for k in actual_results.keys()|expected_results.keys()
                      if actual_results.get(k)!=expected_results.get(k))
    raise SystemExit('Retained diagnostic does not reproduce: '+', '.join(mismatches))
with output.open('x',encoding='utf-8',newline='\n') as target:
    target.write(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
print(json.dumps({'reproduced':True,'compared_report_fields':len(actual_results),
                  'source_archives':len(validation),'stored_metrics_checked':480,
                  'A_threshold_disagreements':len(cross),
                  'A_DA_difference_pp':pair_summary['A']['da_difference_pp'],
                  'release_acceptance_passed':False},indent=2))
