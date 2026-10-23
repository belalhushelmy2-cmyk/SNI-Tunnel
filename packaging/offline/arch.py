#!/usr/bin/env python3
"""Build Arch binary and offline full dependency closure from official sync DBs."""
import argparse, base64, concurrent.futures, hashlib, json, os, shutil, subprocess, tarfile, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser(); p.add_argument('--assets',type=Path,required=True); p.add_argument('--output',type=Path,required=True); a=p.parse_args()
a.assets=a.assets.resolve(); a.output=a.output.resolve(); a.output.mkdir(parents=True,exist_ok=True)
work=ROOT/'target/offline-arch/2.1.1'; work.mkdir(parents=True,exist_ok=True)
build=work/'build'; build.mkdir(exist_ok=True)
archive=a.assets/'sni-tunnel-v2_2.1.1_amd64.tar.gz'
text=(ROOT/'packaging/arch/PKGBUILD').read_text().replace('2.0.1','2.1.1').replace('fee9a3bc2abebd8e2479dc19bee7b82167ea8a6e06f65154be3d62ad60840a82',hashlib.sha256(archive.read_bytes()).hexdigest())
(build/'PKGBUILD').write_text(text); shutil.copy2(archive,build/archive.name)
cmd=['makepkg','--nodeps','--cleanbuild','--noconfirm','--force']
if os.getuid()==0:
 subprocess.run(['chown','-R','nobody:nobody',str(build)],check=True)
 cmd=['runuser','-u','nobody','--',*cmd]
subprocess.run(cmd,cwd=build,check=True)
app=build/'sni-tunnel-v2-bin-2.1.1-1-x86_64.pkg.tar.zst'
assert app.is_file(), 'Expected current-version Arch package is missing'
shutil.copy2(app,a.output/app.name); shutil.copy2(build/'PKGBUILD',a.output/'PKGBUILD')
root=work/'bundle'; packages=root/'packages'; packages.mkdir(parents=True,exist_ok=True)
shutil.copy2(app,packages/app.name)
for pattern in ('SNI-Tunnel-V2-2.1.1-x86_64.AppImage','sni-tunnel-v2_2.1.1_amd64.tar.gz'):
 for asset in a.assets.glob(pattern): shutil.copy2(asset,root/asset.name)
db=work/'db'; (db/'local').mkdir(parents=True,exist_ok=True); (db/'sync').mkdir(exist_ok=True)
metadata={}
for repo in ('core','extra'):
 source=Path('/var/lib/pacman/sync')/(repo+'.db'); shutil.copy2(source,db/'sync'/source.name)
 with tarfile.open(source) as tar:
  for member in tar:
   if not member.name.endswith('/desc'): continue
   sections=tar.extractfile(member).read().decode().strip().split('\n\n'); d={}
   for section in sections:
    lines=section.splitlines(); d[lines[0].strip('%')]=lines[1:]
   metadata[d['NAME'][0]]=d
conf=work/'pacman.conf'; conf.write_text(f'[options]\nArchitecture = x86_64\nDBPath = {db}\nSigLevel = Required DatabaseOptional\n'+''.join(f'[{r}]\nServer = https://geo.mirror.pkgbuild.com/{r}/os/x86_64\n' for r in ('core','extra')))
deps=['glibc','gcc-libs','fontconfig','libxkbcommon','libxkbcommon-x11','libxcb','libx11','libxcursor','libxrandr','libxi','wayland','python','iproute2','nftables','libnotify','polkit','ca-certificates','kmod','glib2','dbus','hyprpolkitagent','wl-clipboard','xclip','ttf-dejavu']
lines=subprocess.check_output(['pacman','--config',str(conf),'-Sp','--noconfirm','--print-format','%n %l',*deps],text=True).splitlines()
records=[]
def download(line):
 name,url=line.split(' ',1); d=metadata[name]; dest=packages/d['FILENAME'][0]
 expected=d['SHA256SUM'][0]
 if not dest.exists() or hashlib.sha256(dest.read_bytes()).hexdigest()!=expected:
  cached=Path('/var/cache/pacman/pkg')/dest.name
  if cached.exists() and hashlib.sha256(cached.read_bytes()).hexdigest()==expected: shutil.copy2(cached,dest)
  else:
   last=None
   for host in ('https://geo.mirror.pkgbuild.com','https://mirrors.kernel.org/archlinux'):
    try:
     with urllib.request.urlopen(url.replace('https://geo.mirror.pkgbuild.com',host),timeout=120) as stream, dest.open('wb') as f: shutil.copyfileobj(stream,f)
     assert hashlib.sha256(dest.read_bytes()).hexdigest()==expected, name+' checksum mismatch'
     last=None; break
    except Exception as e: last=e
   if last: raise last
 sig=dest.with_name(dest.name+'.sig'); sig.write_bytes(base64.b64decode(d['PGPSIG'][0]))
 subprocess.run(['gpgv','--keyring','/etc/pacman.d/gnupg/pubring.gpg',str(sig),str(dest)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
 return {'name':name,'version':d['VERSION'][0],'file':dest.name,'sha256':expected,'url':url}
with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
 for record in pool.map(download,lines): records.append(record); print('Verified',record['name'],flush=True)
(root/'DEPENDENCIES.json').write_text(json.dumps(records,indent=2)+'\n')
subprocess.run(['repo-add',str(packages/'sni-offline.db.tar.gz'),*[str(x) for x in sorted(packages.glob('*.pkg.tar.zst'))]],check=True,stdout=subprocess.DEVNULL)
subprocess.run(['python3',str(Path(__file__).with_name('finish.py')),str(root),'arch',str(a.output)],check=True)
print('Bundled',len(records),'signed official dependencies')
