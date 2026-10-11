"""Standalone pure cap/material falsification screen. Python standard library only.
Run: python -B angular_feasibility.py result.json
Fixed geometry below; no Radiance, fitted parameter, native or annual claim.
"""
import functools, hashlib, json, math as m, pathlib, sys
PI=m.pi; A=m.radians(.533)/2; W=(.265074126,.670114631,.064811243)
RGB=(.7,1.,.4); ORDERS=(32,64,128,256)
NORMALS=((0,0,1),(0,0,-1),(1,0,0),(0,1,0),(0,2**-.5,2**-.5),(0,2**-.5,-2**-.5))
def lux(v): return 179*m.fsum(a*b for a,b in zip(v,W))
def sha(p): return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
@functools.lru_cache(None)
def gauss(n):
    out=[]
    for i in range(1,n+1):
        x=m.cos(PI*(i-.25)/(n+.5))
        for _ in range(30):
            p,q=1.,0.
            for j in range(1,n+1): p,q=((2*j-1)*x*p-(j-1)*q)/j,p
            dp=n*(x*p-q)/(x*x-1); dx=p/dp; x-=dx
            if abs(dx)<2e-16: break
        out.append((x,2/((1-x*x)*dp*dp)))
    return tuple(out)
def glass(c):
    n=1.52; t=.6; r=((n-1)/(n+1))**2; q=(1-r)**2
    k=2*t/(q+m.sqrt(q*q+4*r*r*t*t))
    ci=m.sqrt(1-(1-c*c)/(n*n)); att=k**(1/ci)
    rs=((c-n*ci)/(c+n*ci))**2; rp=((n*c-ci)/(n*c+ci))**2
    return sum((1-v)**2*att/(1-v*v*att*att) for v in (rs,rp))/2
def half_phi(a,b,z):
    e=m.asin(z)
    q=m.sin((b+a-e)/2)*m.sin((b-a+e)/2)/(m.cos(a)*m.cos(e))
    assert q>=-1e-20
    return 2*m.asin(m.sqrt(max(0,q)))
def integral(a,b,n,order,tilt=None,cut=None):
    lo=max(0.,m.sin(a-b)); hi=m.sin(a+b)
    if cut is not None: hi=min(hi,m.sin(cut))
    if hi<=lo: return 0.
    splits=[lo,hi]
    if n[0]==0 and n[1]!=0:
        k=n[2]/n[1]; aa=m.cos(a)**2*(1+k*k)+m.sin(a)**2
        q=m.cos(b)*m.sin(a); disc=q*q-aa*(m.sin(a)**2-m.sin(b)**2)
        if disc>=0:
            splits += [z for z in ((q-m.sqrt(disc))/aa,(q+m.sqrt(disc))/aa) if lo<z<hi]
    if cut is not None:
        z=m.cos(b)*m.sin(cut)/m.cos(a-cut)
        if lo<z<hi: splits.append(z)
    def f(z):
        r=m.sqrt(1-z*z); p=half_phi(a,b,z)
        if cut is not None:
            q=z/(r*m.tan(cut))
            p=min(p,m.acos(min(1.,max(-1.,q))))
        if tilt is not None:
            return p*m.fsum(w*r*m.cos(p*x)*glass(abs(m.cos(tilt)*r*m.cos(p*x)+m.sin(tilt)*r*m.sin(p*x))) for x,w in gauss(order))
        if n[0]==1: return 2*r*m.sin(p)
        if n[1]==0: return 2*p*max(0,n[2]*z)
        q=-n[2]*z/(n[1]*r)
        if q>=1: return 0.
        lower=max(-p,m.asin(max(-1.,q)))
        if lower>=p: return 0.
        return n[1]*r*(m.cos(lower)-m.cos(p))+n[2]*z*(p-lower)
    total=[]; splits=sorted(set(splits))
    for lo,hi in zip(splits,splits[1:]):
        for x,w in gauss(order):
            th=(x+1)*PI/4; z=lo+(hi-lo)*m.sin(th)**2
            total.append(w*PI/4*(hi-lo)*m.sin(2*th)*f(z))
    return m.fsum(total)
def boundary(a):
    q=m.sqrt(m.sin(A-a)*m.sin(A+a)); ph=m.atan2(q,-m.sin(a)*m.cos(A)); g=m.atan2(q,m.cos(A))
    c=ph*m.sin(A)**2; d=m.sin(ph)*m.sin(A)*m.cos(A)
    return (c*m.cos(a)+d*m.sin(a),c*m.sin(a)-d*m.cos(a)+g)
def ratio_error(a,b):
    return abs(a-b)/abs(b) if b else (0. if a==0 else 1e300)
def run():
    rows=[]; maps=[]; convergence=[]
    for mul in (-.95,-.5,0.,.5,.95):
        a=A*mul; mx=integral(a,A,NORMALS[2],256); mz=integral(a,A,NORMALS[0],256)
        bx,bz=boundary(a)
        assert abs(mx-bx)<=max(1e-14,1e-8*abs(mx)) and abs(mz-bz)<=max(1e-14,1e-8*abs(mz))
        norm=m.hypot(mx,mz); ux,uz=(float(format(v/norm,'.17g')) for v in (mx,mz)); unit=m.hypot(ux,uz); ux/=unit; uz/=unit
        ma=m.atan2(uz,ux); beta=min(A,m.asin(uz)/2)
        diameter=float(format(m.degrees(2*beta),'.17g')); beta=m.radians(diameter)/2
        omega=2*PI*(1-m.cos(beta)); scale=norm/omega
        brightness=10000/(lux(RGB)*mx); original=[v*brightness for v in RGB]
        mapped=[float(format(v*scale,'.17g')) for v in original]
        assert beta>1e-6 and ma-beta>0
        assert max(abs(mapped[c]*omega*(ux if k==0 else uz)-original[c]*v) for c in range(3) for k,v in enumerate((mx,mz))) < 1e-8
        maps.append(dict(elevation_alpha=mul,original_rgb=original,moment_xz=[mx,mz],boundary_xz=[bx,bz],mapped_direction=[ux,0.,uz],mapped_diameter_degrees=diameter,mapped_rgb=mapped))
        cases=[('G'+str(i),n,None,None) for i,n in enumerate(NORMALS)]
        if mul in (-.95,0.,.95):
            cut=(max(0,a-A)+a+A)/2
            cases += [('M0',NORMALS[2],None,None),('M1',NORMALS[2],0.,None),('M2',NORMALS[2],m.radians(80),None),('M3',NORMALS[2],0.,cut)]
        for name,n,tilt,cut in cases:
            vals=[]
            for order in ORDERS:
                oo=integral(a,A,n,order,tilt,cut); mm=integral(ma,beta,n,order,tilt,cut)
                vals.append(([v*oo for v in original],[v*mm for v in mapped]))
            vis=cut is None or uz/ux<=m.tan(cut)
            tr=1. if tilt is None else glass(abs(m.cos(tilt)*ux))
            nc=[v*omega*max(0,n[0]*ux+n[2]*uz)*tr*vis for v in mapped]
            oo,mm=vals[-1]; ls=[lux(v) for v in (oo,mm,nc)]
            conv=max(ratio_error(vals[j][k][c],vals[j+1][k][c]) for j in (1,2) for k in (0,1) for c in range(3))
            convergence.append(conv)
            row=dict(case=name,elevation_alpha=mul,original_rgb=oo,mapped_physical_rgb=mm,mapped_centroid_rgb=nc,lux_original_physical_centroid=ls,quadrature_rgb=vals,convergence_relative=conv,converged=conv<=.001)
            row['one_percent_pass']=[all(ratio_error(v[c],oo[c])<=.01 for c in range(3)) for v in (mm,nc)]
            row['five_lx_five_percent_pass']=[abs(v-ls[0])<=max(5,.05*ls[0]) for v in ls[1:]]
            if name.startswith('M'): row['outgoing_lambertian_radiance_rgb']=[[.5*v/PI for v in rgb] for rgb in (oo,mm,nc)]
            rows.append(row)
    assert len(rows)==42 and all(m.isfinite(v) and v>=0 for r in rows for v in r['lux_original_physical_centroid'])
    return dict(schema='greenlens.moment-angular-feasibility.v1',native_stages=0,source_sha256=sha(__file__),maps=maps,rows=rows,convergence_pass=all(r['converged'] for r in rows),maximum_convergence_relative=max(convergence),failed_one_percent=[[r['elevation_alpha'],r['case'],j] for r in rows for j,p in enumerate(r['one_percent_pass']) if not p],failed_five_lx_five_percent=[[r['elevation_alpha'],r['case'],j] for r in rows for j,p in enumerate(r['five_lx_five_percent_pass']) if not p],general_method6_eligible=False,reference_family_qualified=False,accuracy_accepted=False,scope='Surface/angular falsification only; original finite direct can restore final direct, not generally reflected surface illumination. No full room failure inferred.')
if __name__=='__main__':
    target=pathlib.Path(sys.argv[1]); assert not target.exists()
    result=run(); target.write_text(json.dumps(result,indent=2,allow_nan=False)+'\n',encoding='utf-8')
    print(json.dumps({k:result[k] for k in ('native_stages','convergence_pass','maximum_convergence_relative','failed_one_percent','failed_five_lx_five_percent')}))
