"""Independent original synthetic checks for the R1 ASCII/gzip research collector.
No Radiance, retained commands, or scientific output is executed.
"""
import argparse, copy, gzip, hashlib, importlib.util, io, json, math, os
import pathlib, tempfile, time, unittest, uuid
from unittest import mock

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location('reference_under_test', HERE/'reference_stream.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
ROOT = pathlib.Path(os.environ['LOCALAPPDATA'])/'GreenLens'/'verification'/('reference stream guards '+uuid.uuid4().hex+' 空間')
ROOT.mkdir(parents=True)
H = lambda b: hashlib.sha256(b).hexdigest()

def payload(N=3, sensors=2, crlf=False):
    rows=[]
    for block in range(2):
        for sensor in range(sensors):
            for sample in range(N):
                rows.append(' '.join(format(1+block*100+sensor*10+sample+c/8,'.17g') for c in range(6)))
    return (('\r\n' if crlf else '\n').join(rows)+('\r\n' if crlf else '\n')).encode('ascii')

def summary(raw=None,N=3,sensors=2,scale=100):
    return m.parse_rows(io.BytesIO(payload(N,sensors) if raw is None else raw),N,sensors,scale)

class StreamTests(unittest.TestCase):
    def setUp(self):
        self.root=ROOT/(self.id().split('.')[-1]+'-'+uuid.uuid4().hex[:8]);self.root.mkdir()
    def reject_parse(self,raw,**kw):
        with self.assertRaises((ValueError,OverflowError)):
            m.parse_rows(io.BytesIO(raw),kw.pop('N',1),kw.pop('sensors',1),kw.pop('scale',100),**kw)
    def write(self,raw=None,name='rays.rgb'):
        p=self.root/name;p.write_bytes(payload() if raw is None else raw);return p
    def archive(self,prune=False):
        raw=self.write();return raw,m.archive_completed(self.root,raw.name,3,2,100,prune=prune)
    def test_non_gray_block_sensor_order_and_pi(self):
        raw=payload();s=summary(raw)
        self.assertEqual((s['rows'],s['bytes'],s['sha256']),(12,len(raw),H(raw)))
        for b in range(2):
            for sensor in range(2):
                for c in range(6):
                    expected=math.pi*math.fsum(1+b*100+sensor*10+i+c/8 for i in range(3))/3/100
                    self.assertAlmostEqual(s['block_rgb'][b][sensor][c],expected,places=14)
    def test_original_crlf_bytes_preserved_not_normalized(self):
        raw=payload(crlf=True);a=summary(raw);b=summary(payload())
        self.assertEqual(a['block_rgb'],b['block_rgb']);self.assertNotEqual(a['sha256'],b['sha256'])
        self.assertEqual(a['sha256'],H(raw))
    def test_compensated_sum_retains_small_terms(self):
        raw=('\n'.join(' '.join([str(v)]*6) for v in [1e16,1,1,1e16,1,1])+'\n').encode()
        a=summary(raw,3,1);expected=math.pi*(math.fsum([1e16,1,1])/3)/100
        self.assertEqual(a['block_rgb'][0][0],[expected]*6)
    def test_zero_is_exact_and_negative_zero_accepted(self):
        a=summary(b'-0 0e99 +0.0 .0 0. -0e-999\n'*2,1,1)
        self.assertTrue(all(x==0 for b in a['block_rgb'] for row in b for x in row))
    def test_n_sensor_scale_type_and_domain(self):
        for key,bad in [('N',True),('N',0),('N',65537),('N',1.0),('sensors',True),('sensors',0),('sensors',25),('scale',True),('scale',0),('scale',-1),('scale',float('nan')),('scale',float('inf'))]:
            with self.subTest(key=key,bad=bad):self.reject_parse(b'0 0 0 0 0 0\n'*2,**{key:bad})
    def test_malformed_numeric_tokens(self):
        for token in [b'NaN',b'Infinity',b'inf',b'0x1',b'1_0',b'1,0',b'1e',b'.',b'--1',b'1\x00',b'\xff']:
            with self.subTest(token=repr(token)):self.reject_parse(token+b' 0 0 0 0 0\n'+b'0 0 0 0 0 0\n')
    def test_nonzero_underflow_and_negative_rejected(self):
        for token in [b'1e-999',b'-1e-999',b'-1',b'-1e-100',b'1e999']:
            with self.subTest(token=token):self.reject_parse(token+b' 0 0 0 0 0\n'+b'0 0 0 0 0 0\n')
    def test_truncated_extra_and_wrong_column_rows(self):
        for raw in [b'',b'0 0 0 0 0 0\n',b'0 0 0 0 0 0\n'*3,b'0 0 0 0 0\n'*2,b'0 0 0 0 0 0 0\n'*2,b'0 0 0 0 0 0\n0 0 0 0 0 0']:
            with self.subTest(raw=raw):self.reject_parse(raw)
    def test_lone_cr_and_oversized_row(self):
        self.reject_parse(b'0 0\r 0 0 0 0\n'*2)
        self.reject_parse(b'0'*1020+b' 0 0 0 0 0\n'*2)
    def test_exact_row_and_total_byte_bound(self):
        line=b'0'*1013+b' 0 0 0 0 0\n';self.assertEqual(len(line),1024)
        raw=line*2;s=m.parse_rows(io.BytesIO(raw),1,1,100,len(raw));self.assertEqual(s['bytes'],len(raw))
        self.reject_parse(raw,max_bytes=len(raw)-1)
    def test_sum_and_normalized_overflow(self):
        self.reject_parse(b'1e308 0 0 0 0 0\n'*4,N=2)
        self.reject_parse(b'1e308 0 0 0 0 0\n'*2,scale=1e-300)
    def test_deadline_no_success(self):
        self.reject_parse(b'0 0 0 0 0 0\n'*2,end=time.monotonic()-1)
    def test_archive_exact_roundtrip_and_success_prune(self):
        raw,r=self.archive(prune=True)
        self.assertFalse(raw.exists());self.assertTrue(r['uncompressed_duplicate_removed'])
        self.assertTrue(r['independent_exact_decompression_verified'])
        gz=self.root/r['archive_name'];self.assertEqual(gzip.decompress(gz.read_bytes()),payload())
        self.assertEqual(r['gzip_sha256'],H(gz.read_bytes()));self.assertEqual(m.read_archive(self.root,r),r['summary'])
    def test_deterministic_gzip_in_fresh_directory(self):
        _,a=self.archive();second=self.root/'other';second.mkdir();(second/'rays.rgb').write_bytes(payload())
        b=m.archive_completed(second,'rays.rgb',3,2,100)
        self.assertEqual((a['gzip_bytes'],a['gzip_sha256']),(b['gzip_bytes'],b['gzip_sha256']))
        header=(self.root/a['archive_name']).read_bytes()[:10]
        self.assertEqual(header[3]&8,0);self.assertEqual(header[4:8],b'\0'*4)
    def test_partial_raw_never_pruned_or_archived(self):
        p=self.write(b'0 0 0 0 0 0\n')
        with self.assertRaises(ValueError):m.archive_completed(self.root,p.name,3,2,100,prune=True)
        self.assertEqual(p.read_bytes(),b'0 0 0 0 0 0\n');self.assertFalse((self.root/(p.name+'.gz')).exists())
    def test_existing_archive_no_overwrite(self):
        raw,r=self.archive();old=(self.root/r['archive_name']).read_bytes()
        with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,prune=True)
        self.assertTrue(raw.exists());self.assertEqual((self.root/r['archive_name']).read_bytes(),old)
    def test_owned_name_path_and_directory_guards(self):
        self.write()
        for name in ['../outside','..','.','',str(self.root/'rays.rgb'),'child/rays.rgb','child\\rays.rgb']:
            with self.subTest(name=name),self.assertRaises((ValueError,FileNotFoundError)):m.owned_file(self.root,name)
        (self.root/'folder').mkdir()
        with self.assertRaises(ValueError):m.owned_file(self.root,'folder')
    def test_file_cap_and_peak_reserve_before_archive(self):
        raw=self.write();size=raw.stat().st_size
        with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,file_cap=size-1,prune=True)
        with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,scratch_cap=2*size+65535,prune=True)
        self.assertTrue(raw.exists());self.assertFalse((self.root/(raw.name+'.gz')).exists())
    def test_owned_sibling_counts_toward_cap(self):
        raw=self.write();(self.root/'retained.log').write_bytes(b'x'*1000)
        with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,scratch_cap=raw.stat().st_size+999,prune=True)
        self.assertTrue(raw.exists())
    def test_gzip_crc_truncation_and_extra_member_rejected(self):
        _,r=self.archive();p=self.root/r['archive_name'];original=p.read_bytes()
        badcrc=bytearray(original);badcrc[-8]^=1
        for bad in [original[:-1],bytes(badcrc),original+gzip.compress(b'0 0 0 0 0 0\n',mtime=0),original+b'x']:
            p.write_bytes(bad)
            with self.subTest(size=len(bad)),self.assertRaises((ValueError,OSError,EOFError)):
                m.verify_gzip(p,r['raw_sha256'],r['raw_bytes'])
        p.write_bytes(original)
    def test_gzip_declared_size_hash_and_expansion_bound(self):
        _,r=self.archive();p=self.root/r['archive_name']
        for digest,size,cap in [(r['raw_sha256'],r['raw_bytes']-1,m.FILE_LIMIT),(r['raw_sha256'],r['raw_bytes']+1,m.FILE_LIMIT),('0'*64,r['raw_bytes'],m.FILE_LIMIT),(r['raw_sha256'],r['raw_bytes'],r['raw_bytes']-1)]:
            with self.subTest(size=size,cap=cap),self.assertRaises(ValueError):m.verify_gzip(p,digest,size,cap)
    def test_receipt_corruption_rejected(self):
        _,r=self.archive()
        changes=[('schema','other'),('independent_exact_decompression_verified',1),('raw_sha256','0'*64),('raw_bytes',r['raw_bytes']+1),('gzip_sha256','0'*64),('gzip_bytes',r['gzip_bytes']+1)]
        for key,value in changes:
            bad=copy.deepcopy(r);bad[key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):m.read_archive(self.root,bad)
        bad=copy.deepcopy(r);bad['summary']['block_rgb'][0][0][0]+=1
        with self.assertRaises(ValueError):m.read_archive(self.root,bad)
    def test_verification_failure_preserves_raw(self):
        raw=self.write();before=raw.read_bytes()
        with mock.patch.object(m,'verify_gzip',side_effect=ValueError('synthetic decompression fault')):
            with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,prune=True)
        self.assertEqual(raw.read_bytes(),before);self.assertTrue((self.root/(raw.name+'.gz')).exists())
    def test_same_length_source_mutation_prevents_prune(self):
        raw=self.write();verify=m.verify_gzip
        def mutate(*args,**kwargs):
            value=verify(*args,**kwargs);data=raw.read_bytes();raw.write_bytes(b'2'+data[1:]);return value
        with mock.patch.object(m,'verify_gzip',side_effect=mutate):
            with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,prune=True)
        self.assertTrue(raw.exists());self.assertTrue(raw.read_bytes().startswith(b'2'))
    def test_expired_archive_preserves_raw(self):
        raw=self.write()
        with self.assertRaises(ValueError):m.archive_completed(self.root,raw.name,3,2,100,end=time.monotonic()-1,prune=True)
        self.assertTrue(raw.exists())
    def test_component_assembly_non_gray_solar_only(self):
        full=summary(N=1,sensors=1);black=copy.deepcopy(full)
        full['block_rgb']=[[[1,2,3,10,20,30]],[[3,4,5,12,22,32]]]
        black['block_rgb']=[[[900,800,700,1,2,3]],[[600,500,400,2,3,4]]]
        a=m.assemble(full,black,[[.25,.5,.75]])
        self.assertEqual(a['block_rows'][0][0]['reflected_solar_rgb'],[9,18,27])
        self.assertEqual(a['block_rows'][0][0]['total_rgb'],[10.25,20.5,30.75])
        self.assertEqual(a['mean_rows'][0]['diffuse_rgb'],[2,3,4])
        self.assertEqual(a['mean_rows'][0]['reflected_solar_rgb'],[9.5,18.5,27.5])
        self.assertEqual(a['mean_rows'][0]['total_rgb'],[11.75,22,32.25])
        expected=179*math.fsum(x*y for x,y in zip([11.75,22,32.25],[.265074126,.670114631,.064811243]))
        self.assertAlmostEqual(a['mean_rows'][0]['total_lux'],expected,places=11)
        self.assertTrue(a['completed']);self.assertFalse(a['reference_family_qualified']);self.assertFalse(a['accuracy_accepted'])
    def test_negative_reflection_not_hidden_by_positive_lux(self):
        full=summary(N=1,sensors=1);black=copy.deepcopy(full)
        full['block_rgb']=[[[1,2,3,1,200,300]],[[1,2,3,1,200,300]]]
        black['block_rgb']=[[[0,0,0,2,0,0]],[[0,0,0,0,0,0]]]
        a=m.assemble(full,black,[[10,10,10]])
        self.assertTrue(a['calculations_finished']);self.assertFalse(a['completed']);self.assertEqual(a['negative_sensor_blocks'],[[0,0]])
        self.assertEqual(a['block_rows'][0][0]['reflected_solar_rgb'],[-1,200,300])
        self.assertGreater(a['block_rows'][0][0]['reflected_solar_lux'],0)
    def test_paired_shapes_and_finite_data_rejected(self):
        a=summary(N=1,sensors=1)
        for key,value in [('N',2),('sensors',2),('blocks',1),('scale',10000)]:
            b=copy.deepcopy(a);b[key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):m.assemble(a,b,[[0,0,0]])
        for finite in [[],[[0,0]],[[0,0,-1]],[[0,0,float('nan')]],[[0,0,float('inf')]],[[0,0,True]]]:
            with self.subTest(finite=finite),self.assertRaises(ValueError):m.assemble(a,a,finite)
    def test_fixed_comparison_each_component_denominator(self):
        row=lambda vals:dict(sensor_id='s000001',zip_marker=False,**dict(zip(('diffuse_lux','reflected_solar_lux','total_lux'),vals)))
        a=[row([0,101,200])];b=[row([.1,100,205])]
        out=m.compare(a,b,label='original')
        self.assertEqual([v['passed'] for v in out],[True,True,False]);self.assertEqual([v['allowed'] for v in out],[.1,1,2.05])
        self.assertEqual(len(out),3);self.assertEqual(out[-1]['absolute_difference'],5)
        with self.assertRaises(ValueError):m.compare(a,[])
        bad=copy.deepcopy(b);bad[0]['sensor_id']='s000002'
        with self.assertRaises(ValueError):m.compare(a,bad)
        bad=copy.deepcopy(b);bad[0]['total_lux']=float('nan')
        with self.assertRaises(ValueError):m.compare(a,bad)

    def report_fixture(self):
        import reference_report as R
        plan=json.loads((HERE/'proposal.json').read_text(encoding='utf-8'))
        raw=json.dumps(plan,separators=(',',':')).encode();(self.root/'proposal.json').write_bytes(raw)
        d={'proposal_sha256':H(raw),'finite_sha256':{}}
        for key in ['A4116','B8512']:
            folder=self.root/key;folder.mkdir();finite=json.dumps([[0,0,0]]*24).encode();(folder/'finite-original.json').write_bytes(finite);d['finite_sha256'][key]=H(finite)
            for N in [16384,32768,65536]:
                for variant in ['base','scale10000','weight1e10']:
                    for ab in [0,8,9]:
                        name=f'{key}-{variant}-N{N}-ab{ab}.rgb.archive.json'
                        (folder/name).write_text(json.dumps({'test_name':name}),encoding='utf-8')
        (self.root/'execution-declaration.json').write_text(json.dumps(d),encoding='utf-8')
        def fake_read(folder,receipt,end):
            name=receipt['test_name'];N=int(name.split('-N')[1].split('-')[0]);scale=10000 if '-scale10000-' in name else 100
            rgb=[0,0,0,1,2,3] if '-ab0.' in name else [1,2,3,10,20,30]
            return {'N':N,'sensors':24,'blocks':2,'scale':scale,'block_rgb':[[rgb.copy() for _ in range(24)] for _ in range(2)]}
        return R,fake_read
    def test_report_R1_keeps_all1008_component_checks(self):
        R,fake=self.report_fixture()
        with mock.patch.object(R.S,'read_archive',side_effect=fake):r=R.collect(self.root,'R1','base',time.monotonic()+10)
        self.assertEqual(len(r['cases']),2);self.assertEqual(len(r['checks']),1008);self.assertTrue(r['all_declared_checks_passed'])
    def test_report_R1_rejects_coherently_wrong_archive_shape(self):
        R,fake=self.report_fixture()
        for key,value in [('N',1),('sensors',23),('blocks',3),('scale',1)]:
            def bad_read(folder,receipt,end):
                a=fake(folder,receipt,end);a[key]=value
                if key=='sensors':a['block_rgb']=[b[:value] for b in a['block_rgb']]
                return a
            with self.subTest(key=key),mock.patch.object(R.S,'read_archive',side_effect=bad_read):
                with self.assertRaises(ValueError):R.collect(self.root,'R1','base',time.monotonic()+10)
    def test_report_R2_total_predicate_does_not_gate_descriptive_components(self):
        R,fake=self.report_fixture()
        def changed(folder,receipt,end):
            a=fake(folder,receipt,end)
            if '-weight1e10-' in receipt['test_name'] and '-ab0.' not in receipt['test_name']:
                for block in a['block_rgb']:
                    for row in block:row[0]+=1;row[3]-=1
            return a
        with mock.patch.object(R.S,'read_archive',side_effect=changed):r=R.collect(self.root,'R2','weight1e10',time.monotonic()+10)
        self.assertEqual(len(r['checks']),48);self.assertTrue(r['all_declared_checks_passed'])
        self.assertTrue(all(x['component']=='total_lux' for x in r['checks']))
    def test_report_R2_scale_checks_all1152_full_control_channels(self):
        R,fake=self.report_fixture()
        with mock.patch.object(R.S,'read_archive',side_effect=fake):r=R.collect(self.root,'R2','scale10000',time.monotonic()+10)
        self.assertEqual(len(r['checks']),1152);self.assertTrue(r['all_declared_checks_passed'])
        def mismatch(folder,receipt,end):
            a=fake(folder,receipt,end)
            if receipt['test_name'].startswith('B8512-scale10000-') and '-ab0.' in receipt['test_name']:a['block_rgb'][1][23][0]=1e-10
            return a
        with mock.patch.object(R.S,'read_archive',side_effect=mismatch):r=R.collect(self.root,'R2','scale10000',time.monotonic()+10)
        failed=[c for c in r['checks'] if not c['passed']];self.assertEqual(len(failed),1);self.assertEqual(failed[0]['sensor_id'],'s000024');self.assertEqual(failed[0]['channel'],0)
class Recording(unittest.TextTestResult):
    def __init__(self,*args,**kwargs):super().__init__(*args,**kwargs);self.observed=[]
    def addSuccess(self,test):super().addSuccess(test);self.observed.append({'test':test.id().split('.')[-1],'passed':True})
    def addFailure(self,test,err):super().addFailure(test,err);self.observed.append({'test':test.id().split('.')[-1],'passed':False,'kind':'failure'})
    def addError(self,test,err):super().addError(test,err);self.observed.append({'test':test.id().split('.')[-1],'passed':False,'kind':'error'})

if __name__=='__main__':
    source_names=('reference_stream.py','reference_report.py','proposal.json');source_hashes={n:H((HERE/n).read_bytes()) for n in source_names};before=source_hashes['reference_stream.py'];start=time.monotonic()
    result=unittest.TextTestRunner(verbosity=2,resultclass=Recording).run(unittest.defaultTestLoader.loadTestsFromTestCase(StreamTests))
    after=H((HERE/'reference_stream.py').read_bytes());unchanged=all(H((HERE/n).read_bytes())==h for n,h in source_hashes.items())
    record={'schema':'greenlens.reference-stream-synthetic-checks.v1','scope':'Original synthetic ASCII/gzip/component tests; no Radiance execution or scientific qualification','helper_sha256':before,'source_sha256':source_hashes,'test_sha256':H(pathlib.Path(__file__).read_bytes()),'source_unchanged':unchanged,'tests':result.testsRun,'passed':result.wasSuccessful() and unchanged,'failures':len(result.failures),'errors':len(result.errors),'wall_seconds':time.monotonic()-start,'checks':result.observed,'engine_processes':0,'reference_family_qualified':False}
    (ROOT/'tests.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('Evidence: '+str(ROOT/'tests.json'))
    raise SystemExit(0 if record['passed'] else 1)
