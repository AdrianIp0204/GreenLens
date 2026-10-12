"""Fixed R1/R2 assembly only; imports the reviewed streaming collector."""
import argparse,hashlib,json,pathlib,time
import reference_stream as S

def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def expected(summary,N,scale):
    S.require(summary['N']==N and summary['sensors']==24 and summary['blocks']==2 and summary['scale']==scale,'archive differs from exact declared dimensions/scale')
    return summary
def collect(root,phase,variant,end):
    root=pathlib.Path(root)
    plan=load(root/'proposal.json');declaration=load(root/'execution-declaration.json')
    S.require(hashlib.sha256((root/'proposal.json').read_bytes()).hexdigest()==declaration['proposal_sha256'],'proposal binding')
    cases=[];checks=[];hashes={}
    for case in plan['cases']:
        key=f"{case['alternative']}{case['interval_index']}";folder=root/key
        S.inventory(folder);finite_path=folder/'finite-original.json'
        S.require(S.sha_file(finite_path)[0]==declaration['finite_sha256'][key],'finite binding')
        finite=load(finite_path);assemblies={}
        Ns=(16384,32768,65536) if phase=='R1' else (65536,)
        for N in Ns:
            variants=('base',) if phase=='R1' else ('base',variant)
            for v in variants:
                depths=(8,9) if phase=='R1' else (9,)
                stem=f'{key}-{v}-N{N}'
                control_name=f'{stem}-ab0.rgb.archive.json';cp=folder/control_name
                scale=10000 if v=='scale10000' else 100
                control=expected(S.read_archive(folder,load(cp),end),N,scale);hashes[f'{key}/{control_name}']=S.sha_file(cp)[0]
                for ab in depths:
                    full_name=f'{stem}-ab{ab}.rgb.archive.json';fp=folder/full_name
                    full=expected(S.read_archive(folder,load(fp),end),N,scale);hashes[f'{key}/{full_name}']=S.sha_file(fp)[0]
                    output=S.assemble(full,control,finite)
                    output['variant']=v;output['ab']=ab
                    assemblies[(v,N,ab)]=output
        if phase=='R1':
            for ab in (8,9):
                for lower,higher in ((16384,32768),(32768,65536)):
                    checks.extend(S.compare(assemblies[('base',lower,ab)]['mean_rows'],assemblies[('base',higher,ab)]['mean_rows'],label=f'{key}/N{lower}-{higher}/ab{ab}'))
                highest=assemblies[('base',65536,ab)]
                checks.extend(S.compare(highest['block_rows'][0],highest['block_rows'][1],highest['mean_rows'],label=f'{key}/blocks/ab{ab}'))
            checks.extend(S.compare(assemblies[('base',65536,8)]['mean_rows'],assemblies[('base',65536,9)]['mean_rows'],label=f'{key}/depth8-9'))
        else:
            base=assemblies[('base',65536,9)];test=assemblies[(variant,65536,9)]
            if variant=='scale10000':
                # All normalized full/control source bins, both blocks, everyRGB.
                for ab in (9,0):
                    left=S.read_archive(folder,load(folder/f'{key}-base-N65536-ab{ab}.rgb.archive.json'),end)
                    right=S.read_archive(folder,load(folder/f'{key}-{variant}-N65536-ab{ab}.rgb.archive.json'),end)
                    for b in range(2):
                        for i in range(24):
                            for c in range(6):
                                a=left['block_rgb'][b][i][c];v=right['block_rgb'][b][i][c];tol=1e-5*a
                                checks.append({'label':f'{key}/scale/ab{ab}','block':b,'sensor_id':f's{i+1:06d}','channel':c,'left':a,'right':v,'allowed':tol,'absolute_difference':abs(a-v),'passed':a==v if a==0 else abs(a-v)<=tol})
            else:
                # R2's frozen gate is TOTAL with baseline-total denominator.
                # Component values remain in every retained assembly row.
                for a,v in zip(base['mean_rows'],test['mean_rows']):
                    delta=abs(a['total_lux']-v['total_lux']);tol=max(.1,.01*a['total_lux'])
                    checks.append({'label':f'{key}/{variant}','sensor_id':a['sensor_id'],'component':'total_lux','left':a['total_lux'],'right':v['total_lux'],'denominator':a['total_lux'],'absolute_difference':delta,'allowed':tol,'passed':delta<=tol})
        cases.append({'case':key,'assemblies':list(assemblies.values())})
    negatives=[{'case':c['case'],'N':a['N'],'ab':a['ab'],'variant':a['variant'],'sensor_blocks':a['negative_sensor_blocks']} for c in cases for a in c['assemblies'] if not a['completed']]
    passed=not negatives and all(c['passed'] for c in checks)
    S.deadline(end)
    return {'schema':'greenlens.external-primary-reference-check.v1','phase':phase,'variant':variant,'proposal_sha256':declaration['proposal_sha256'],'cases':cases,'checks':checks,'negative_results':negatives,'archive_receipt_sha256':hashes,'calculations_finished':True,'completed':not negatives,'all_declared_checks_passed':passed,'estimator_eligible':False,'reference_family_qualified':False,'accuracy_accepted':False,'release_accepted':False}

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',required=True);p.add_argument('--phase',choices=['R1','R2'],required=True);p.add_argument('--variant',choices=['base','scale10000','weight1e10','depth24','secondary256'],default='base');p.add_argument('--seconds',type=float,required=True);a=p.parse_args()
    r=collect(a.root,a.phase,a.variant,time.monotonic()+S.number(a.seconds,True))
    print(json.dumps(r,allow_nan=False,separators=(',',':')))
