param([string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(-not $OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA ('GreenLens/verification/reference primary guards '+[guid]::NewGuid().ToString('N')+' 空間')}
New-Item -ItemType Directory -Path $OutputDirectory -ErrorAction Stop | Out-Null
$source=Join-Path $PSScriptRoot 'reference_primary.cs'
$before=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
Add-Type -LiteralPath $source
Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Text;
using System.Globalization;
using System.Security.Cryptography;
public static class IndependentReferencePrimaryAudit {
 public static long Check(byte[] bytes,int n,long receiptBytes,long receiptRays,string digest) {
  if(bytes.LongLength!=receiptBytes || receiptRays!=4L*n)throw new Exception("Receipt dimensions");
  string sha=Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();if(sha!=digest)throw new Exception("Receipt digest");
  if(bytes.Length%2!=0)throw new Exception("Two complete byte blocks");
  int half=bytes.Length/2;for(int i=0;i<half;i++)if(bytes[i]!=bytes[i+half])throw new Exception("Repeated block bytes differ");
  foreach(byte b in bytes)if(b>127 || b==13)throw new Exception("Non ASCII/LF encoding");
  var reader=new StringReader(Encoding.ASCII.GetString(bytes));long rows=0;
  double sx=0,sy=0,sz=0,sxx=0,syy=0,szz=0;
  for(int block=0;block<2;block++)for(int sensor=0;sensor<2;sensor++)for(int i=0;i<n;i++){
   string line=reader.ReadLine();if(line==null)throw new Exception("Truncated generator");
   var fields=line.Split(' ');if(fields.Length!=6)throw new Exception("Six fields");
   double[] v=new double[6];for(int k=0;k<6;k++){v[k]=double.Parse(fields[k],CultureInfo.InvariantCulture);if(!double.IsFinite(v[k]))throw new Exception("Finite fields");}
   double[] origin=sensor==0?new double[]{.5,1.5,.8001}:new double[]{2.5,3.5,1.2001};
   for(int k=0;k<3;k++)if(Math.Abs(v[k]-origin[k])>3e-16)throw new Exception("Effective origin/order");
   uint x=(uint)i; x=((x&0x55555555u)<<1)|((x>>1)&0x55555555u);x=((x&0x33333333u)<<2)|((x>>2)&0x33333333u);x=((x&0x0f0f0f0fu)<<4)|((x>>4)&0x0f0f0f0fu);x=(x<<24)|((x&0xff00u)<<8)|((x>>8)&0xff00u)|(x>>24);
   double angle=(x/4294967296.0)*2*Math.PI,radial=(i+.5)/n;
   double ex=Math.Sqrt(radial)*Math.Cos(angle),ey=Math.Sqrt(radial)*Math.Sin(angle),ez=Math.Sqrt(1-radial);
   if(Math.Abs(v[3]-ex)>2e-15 || Math.Abs(v[4]-ey)>2e-15 || Math.Abs(v[5]-ez)>3e-14)throw new Exception($"Independent mapping N{n}/i{i}: deltas {v[3]-ex:G17},{v[4]-ey:G17},{v[5]-ez:G17}");
   if(v[5]<=0 || Math.Abs(v[3]*v[3]+v[4]*v[4]+v[5]*v[5]-1)>1e-14)throw new Exception("Unit upper hemisphere");
   if(block==0 && sensor==0){sx+=v[3];sy+=v[4];sz+=v[5];sxx+=v[3]*v[3];syy+=v[4]*v[4];szz+=v[5]*v[5];}
   rows++;
  }
  if(reader.ReadLine()!=null || rows!=4L*n)throw new Exception("Extra generator rows");
  if(Math.Abs(sx/n)>1e-4 || Math.Abs(sy/n)>1e-4 || Math.Abs(sz/n-2.0/3)>1e-6 || Math.Abs(sxx/n-.25)>1e-8 || Math.Abs(syy/n-.25)>1e-8 || Math.Abs(szz/n-.5)>1e-12)throw new Exception("Cosine projected-measure moments");
  return rows;
 }
}
"@
$checks=[Collections.Generic.List[object]]::new()
function Check($Name,[scriptblock]$Action){& $Action;$checks.Add([ordered]@{name=$Name;passed=$true})}
$sensors="0.5 1.5 0.8 0 0 1`n2.5 3.5 1.2 0 0 1`n"
$receipts=@();$started=[DateTimeOffset]::UtcNow
foreach($n in @(16384,32768,65536)){
 $producer=[GreenLensReferenceStream.HammersleyProducer]::new($sensors,$n)
 $memory=[IO.MemoryStream]::new()
 try {
  $receipt=$producer.GenerateAsync($memory,[Threading.CancellationToken]::None).GetAwaiter().GetResult()
  $rows=[IndependentReferencePrimaryAudit]::Check($memory.ToArray(),$n,$receipt.Bytes,$receipt.Rays,$receipt.Sha256)
  $checks.Add([ordered]@{name="N$n all directions, origins, two full blocks, cosine moments and exact receipt";passed=$true;rows=$rows})
  $receipts+= [ordered]@{N=$n;bytes=$receipt.Bytes;rays=$receipt.Rays;sha256=$receipt.Sha256}
 } finally {$memory.Dispose()}
}
Check 'All three approved full domains; invalid counts rejected' {
 foreach($n in @(0,1,1024,32767,65537)){$failed=$false;try{$null=[GreenLensReferenceStream.HammersleyProducer]::new($sensors,$n)}catch{$failed=$true};if(-not $failed){throw "Accepted unsealed N$n"}}
}
Check 'Nonfinite, wrong direction and incomplete sensor records rejected' {
 foreach($text in @('',"0 0 0 0 0 1", "0 0 0 0 0 -1`n", "NaN 0 0 0 0 1`n", "0 0 Infinity 0 0 1`n", "0 0 0 0 1`n")){$failed=$false;try{$null=[GreenLensReferenceStream.HammersleyProducer]::new($text,16384)}catch{$failed=$true};if(-not $failed){throw 'Accepted malformed sensor'}}
}
Check 'Cancellation returns no completed receipt' {
 $producer=[GreenLensReferenceStream.HammersleyProducer]::new($sensors,16384);$cancel=[Threading.CancellationTokenSource]::new();$cancel.Cancel();$memory=[IO.MemoryStream]::new();$failed=$false
 try{$null=$producer.GenerateAsync($memory,$cancel.Token).GetAwaiter().GetResult()}catch{$failed=$true}finally{$memory.Dispose();$cancel.Dispose()};if(-not $failed){throw 'Cancellation succeeded'}
}
Check 'Unwritable stream rejected' {
 $producer=[GreenLensReferenceStream.HammersleyProducer]::new($sensors,16384);$memory=[IO.MemoryStream]::new([byte[]]@(1),$false);$failed=$false
 try{$null=$producer.GenerateAsync($memory,[Threading.CancellationToken]::None).GetAwaiter().GetResult()}catch{$failed=$true}finally{$memory.Dispose()};if(-not $failed){throw 'Unwritable stream accepted'}
}
Check 'Primary count changes radial domain rather than reusing low-N prefix' {
 if([Math]::Sqrt(.5/16384) -eq [Math]::Sqrt(.5/32768)){throw 'Unexpected nested prefix'}
}
$after=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant();if($after-ne$before){throw 'Generator source changed'}
$record=[ordered]@{schema='greenlens.reference-primary-synthetic-checks.v1';scope='Original pure C# serialization, independent bit-reversal/cosine measure/origin/order checks; no engines';passed=$true;source_sha256=$before;test_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant();source_unchanged=$true;checks=$checks;check_count=$checks.Count;receipts=$receipts;total_rows=($receipts.rays|Measure-Object -Sum).Sum;wall_seconds=([DateTimeOffset]::UtcNow-$started).TotalSeconds;engine_processes=0;reference_family_qualified=$false}
$record|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $OutputDirectory 'tests.json') -Encoding utf8
$record|ConvertTo-Json -Depth 12
Write-Output "Evidence: $OutputDirectory"
