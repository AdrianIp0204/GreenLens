"""Original GreenLens research-only streaming collector, Apache-2.0.

ASCII engine interchange is preserved byte-for-byte in bounded gzip archives.
This module never starts Radiance and never executes retained source/commands.
"""
import argparse, gzip, hashlib, json, math, os, pathlib, re, time

FILE_LIMIT=536870912
SCRATCH_LIMIT=2147483648
NUMBER=re.compile(rb'[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?\Z')
LUX=(.265074126,.670114631,.064811243)

def require(ok,message):
    if not ok:raise ValueError(message)

def integer(value,minimum,maximum):
    require(type(value) is int and minimum<=value<=maximum,'invalid integer')
    return value

def number(value,positive=False):
    require(type(value) in (int,float) and math.isfinite(value) and (value>0 if positive else True),'invalid number')
    return value

def deadline(end):
    if end is not None:require(time.monotonic()<end,'collector deadline exceeded')

def sha_file(path,cap=FILE_LIMIT):
    h=hashlib.sha256();count=0
    with open(path,'rb') as f:
        while data:=f.read(65536):
            count+=len(data);require(count<=cap,'file cap exceeded');h.update(data)
    return h.hexdigest(),count

def owned_file(root,name,exists=True):
    root=pathlib.Path(root).absolute();p=root/name
    require(type(name) is str and name==pathlib.Path(name).name and name not in ('','.','..'),'immediate owned filename required')
    require(not root.is_symlink() and root.is_dir(),'owned root required')
    for q in (root,*root.parents):
        require(not q.is_symlink() and not (getattr(q.stat(),'st_file_attributes',0)&0x400),'reparse ancestor forbidden')
    if p.exists():require(p.is_file() and not p.is_symlink() and not (getattr(p.stat(),'st_file_attributes',0)&0x400),'reparse/nonfile forbidden')
    require(not exists or p.exists(),'missing owned member')
    return p

def inventory(root,file_cap=FILE_LIMIT,scratch_cap=SCRATCH_LIMIT):
    total=0
    for p in pathlib.Path(root).iterdir():
        owned_file(root,p.name)
        n=p.stat().st_size;require(n<=file_cap,'owned single-file cap');total+=n
    require(total<=scratch_cap,'owned scratch cap')
    return total

def parse_rows(stream,N,sensors,scale,max_bytes=FILE_LIMIT,end=None):
    integer(N,1,65536);integer(sensors,1,24);number(scale,True)
    expected=2*sensors*N
    sums=[[[0.]*6 for _ in range(sensors)] for _ in range(2)]
    compensation=[[[0.]*6 for _ in range(sensors)] for _ in range(2)]
    h=hashlib.sha256();count=rows=0
    while line:=stream.readline(1025):
        count+=len(line);require(count<=max_bytes,'ASCII decompressed cap');h.update(line)
        require(len(line)<=1024 and line.endswith(b'\n'),'oversized/truncated ASCII row')
        body=line[:-1]
        if body.endswith(b'\r'):body=body[:-1]
        require(b'\r' not in body and rows<expected,'loneCR/excess rows')
        tokens=body.split();require(len(tokens)==6,'exact six-column row required')
        values=[]
        for token in tokens:
            require(NUMBER.fullmatch(token) is not None,'malformed numeric token')
            v=float(token);require(math.isfinite(v) and v>=0,'nonfinite/negative raw value')
            mantissa=token.lower().split(b'e')[0]
            require(v!=0 or not any(c in b'123456789' for c in mantissa),'nonzero numeric underflow')
            values.append(v)
        b=rows//(sensors*N);s=(rows//N)%sensors
        for c,v in enumerate(values):
            # Neumaier compensated streaming sum, bounded independent of rows.
            old=sums[b][s][c];t=old+v
            compensation[b][s][c]+=(old-t)+v if abs(old)>=abs(v) else (v-t)+old
            sums[b][s][c]=t
            require(math.isfinite(t) and math.isfinite(compensation[b][s][c]),'sum overflow')
        rows+=1
        if not rows%4096:deadline(end)
    require(rows==expected,'truncated primary stream');deadline(end)
    means=[[[math.pi*((sums[b][s][c]+compensation[b][s][c])/N)/scale for c in range(6)] for s in range(sensors)] for b in range(2)]
    require(all(math.isfinite(v) and v>=0 for block in means for row in block for v in row),'normalized overflow')
    return {'N':N,'sensors':sensors,'blocks':2,'scale':scale,'rows':rows,'bytes':count,'sha256':h.hexdigest(),'block_rgb':means}

def verify_gzip(path,expected_sha,expected_bytes,max_bytes=FILE_LIMIT,end=None):
    require(type(expected_bytes) is int and 0<=expected_bytes<=max_bytes,'invalid original size')
    h=hashlib.sha256();count=0
    with gzip.open(path,'rb') as f:
        while block:=f.read(65536):
            count+=len(block);require(count<=expected_bytes and count<=max_bytes,'gzip exceeds sealed size');h.update(block);deadline(end)
    require(count==expected_bytes and h.hexdigest()==expected_sha,'gzip does not reproduce original ASCII')
    return count

def archive_completed(root,name,N,sensors,scale,*,file_cap=FILE_LIMIT,scratch_cap=SCRATCH_LIMIT,end=None,prune=False):
    """Caller must verify successful process first. Never prune failed partials.

    With prune=True only the passed fresh owned name is removed, after complete
    strict parsing and independent exact decompression verification.
    """
    raw=owned_file(root,name);target=owned_file(root,name+'.gz',False)
    require(not target.exists(),'archive already exists');inventory(root,file_cap,scratch_cap)
    with raw.open('rb') as f:summary=parse_rows(f,N,sensors,scale,file_cap,end)
    require(raw.stat().st_size==summary['bytes'],'raw changed during parse')
    require(inventory(root,file_cap,scratch_cap)+summary['bytes']+65536<=scratch_cap,'insufficient archive peak reserve')
    with raw.open('rb') as src,target.open('xb') as out:
        with gzip.GzipFile(filename='',fileobj=out,mode='wb',compresslevel=6,mtime=0) as dst:
            while block:=src.read(65536):
                dst.write(block);deadline(end)
                require(out.tell()<=file_cap,'gzip file cap');require(inventory(root,file_cap,scratch_cap)<=scratch_cap,'archive scratch cap')
    compressed_sha,compressed_bytes=sha_file(target,file_cap)
    verify_gzip(target,summary['sha256'],summary['bytes'],file_cap,end)
    # Detect any source mutation, including same-length replacement after parse.
    require(sha_file(raw,file_cap)==(summary['sha256'],summary['bytes']),'raw changed before verified prune')
    inventory(root,file_cap,scratch_cap);deadline(end)
    receipt={'schema':'greenlens.reference-ascii-gzip.v1','raw_name':name,'archive_name':target.name,'raw_sha256':summary['sha256'],'raw_bytes':summary['bytes'],'gzip_sha256':compressed_sha,'gzip_bytes':compressed_bytes,'gzip_level':6,'gzip_mtime':0,'summary':summary,'independent_exact_decompression_verified':True,'uncompressed_duplicate_removed':False}
    if prune:raw.unlink();receipt['uncompressed_duplicate_removed']=True
    return receipt

def read_archive(root,receipt,end=None):
    require(receipt.get('schema')=='greenlens.reference-ascii-gzip.v1' and receipt.get('independent_exact_decompression_verified') is True,'unverified archive receipt')
    p=owned_file(root,receipt['archive_name']);require(sha_file(p)==(receipt['gzip_sha256'],receipt['gzip_bytes']),'archive digest differs')
    meta=receipt['summary']
    with gzip.open(p,'rb') as f:actual=parse_rows(f,meta['N'],meta['sensors'],meta['scale'],FILE_LIMIT,end)
    require(actual==meta and actual['sha256']==receipt['raw_sha256'] and actual['bytes']==receipt['raw_bytes'],'archive recomputation differs')
    return actual

def lux(rgb):
    require(len(rgb)==3,'RGB shape');v=179*math.fsum(number(rgb[c])*LUX[c] for c in range(3));number(v);return v

def assemble(full,black,finite):
    for k in ('N','sensors','blocks','scale'):require(full[k]==black[k],'paired shape/scale differs')
    s=full['sensors'];require(len(finite)==s,'finite sensor count')
    blocks=[];negative=[]
    for b in range(2):
        rows=[]
        for i in range(s):
            f=full['block_rgb'][b][i];d=black['block_rgb'][b][i];v=finite[i]
            require(len(v)==3 and all(number(x)>=0 for x in v),'finite values')
            reflected=[number(f[3+c]-d[3+c]) for c in range(3)]
            total=[number(f[c]+reflected[c]+v[c]) for c in range(3)]
            if min(reflected)<0 or min(total)<0:negative.append([b,i])
            rows.append({'sensor_index':i,'sensor_id':f's{i+1:06d}','diffuse_rgb':f[:3],'full_solar_rgb':f[3:],'black_direct_rgb':d[3:],'finite_direct_rgb':v,'reflected_solar_rgb':reflected,'total_rgb':total,'diffuse_lux':lux(f[:3]),'reflected_solar_lux':lux(reflected),'total_lux':lux(total)})
        blocks.append(rows)
    means=[]
    for i in range(s):
        row={'sensor_index':i,'sensor_id':f's{i+1:06d}'}
        for k in blocks[0][i]:
            if k.endswith('_rgb'):row[k]=[number((blocks[0][i][k][c]+blocks[1][i][k][c])/2) for c in range(3)]
            elif k.endswith('_lux'):row[k]=number((blocks[0][i][k]+blocks[1][i][k])/2)
        means.append(row)
    return {'N':full['N'],'scale':full['scale'],'block_rows':blocks,'mean_rows':means,'negative_sensor_blocks':negative,'calculations_finished':True,'completed':not negative,'reference_family_qualified':False,'accuracy_accepted':False}

def compare(a,b,denominator=None,label=''):
    denominator=b if denominator is None else denominator
    require(len(a)==len(b)==len(denominator),'comparison cardinality')
    out=[]
    for i,(x,y,z) in enumerate(zip(a,b,denominator)):
        require(x['sensor_id']==y['sensor_id']==z['sensor_id']==f's{i+1:06d}','sensor order')
        for k in ('diffuse_lux','reflected_solar_lux','total_lux'):
            delta=abs(number(x[k])-number(y[k]));tol=max(.1,.01*number(z[k]));number(delta)
            out.append({'label':label,'sensor_id':x['sensor_id'],'component':k,'left':x[k],'right':y[k],'denominator':z[k],'absolute_difference':delta,'allowed':tol,'passed':delta<=tol})
    return out

def main():
    ap=argparse.ArgumentParser();ap.add_argument('mode',choices=['archive','read']);ap.add_argument('--root',required=True);ap.add_argument('--name',required=True);ap.add_argument('--N',type=int);ap.add_argument('--scale',type=float);ap.add_argument('--seconds',type=float,required=True);a=ap.parse_args()
    end=time.monotonic()+number(a.seconds,True)
    if a.mode=='archive':r=archive_completed(a.root,a.name,a.N,24,a.scale,end=end,prune=False)
    else:r=read_archive(a.root,json.loads(owned_file(a.root,a.name).read_text(encoding='utf-8-sig')),end)
    print(json.dumps(r,allow_nan=False,separators=(',',':')))

if __name__=='__main__':main()
