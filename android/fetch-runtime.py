#!/usr/bin/env python3
"""Fetch official, SHA256-pinned Android runtimes; no user data involved."""
from pathlib import Path
import concurrent.futures
import hashlib
import subprocess
import zipfile

ROOT=Path(__file__).resolve().parent
RUNTIMES=[
 ('hev-socks5-tunnel.aar','https://github.com/heiher/hev-socks5-tunnel/releases/download/2.19.1/hev-socks5-tunnel.aar','107958f7a07051386248ba8dba778b63d8dcefdfb2c3665b0ee9a8c93de5f6ca',None),
 ('Xray-android-arm64-v8a.zip','https://github.com/XTLS/Xray-core/releases/download/v26.10.21/Xray-android-arm64-v8a.zip','d81604f35a56c98ea6091142c1c744f4895c7384bc2e1c85cd910612c70794e9','arm64-v8a'),
 ('Xray-android-amd64.zip','https://github.com/XTLS/Xray-core/releases/download/v26.10.21/Xray-android-amd64.zip','58546b1a2711164dc0db9b847fca3d3d6829b2631ab8d6105dc6790a26c80934','x86_64'),
]
def fetch(item):
 name,url,digest,abi=item
 cache=ROOT/'.gradle/runtime';cache.mkdir(parents=True,exist_ok=True)
 archive=cache/name
 if not archive.exists() or hashlib.sha256(archive.read_bytes()).hexdigest()!=digest:
  subprocess.run(['curl','--connect-timeout','15','--max-time','180','--fail','--location','--retry','4','--silent','--show-error',url,'-o',str(archive)],check=True)
 if hashlib.sha256(archive.read_bytes()).hexdigest()!=digest:raise RuntimeError('SHA256 mismatch: '+name)
 if abi:
  target=ROOT/'app/src/main/jniLibs'/abi;target.mkdir(parents=True,exist_ok=True)
  with zipfile.ZipFile(archive) as z:
   binary=z.read('xray')
  if binary[:4]!=b'\x7fELF':raise RuntimeError('Not an ELF executable')
  (target/'libxray.so').write_bytes(binary);(target/'libxray.so').chmod(0o755)
 else:
  target=ROOT/'app/libs';target.mkdir(parents=True,exist_ok=True);(target/name).write_bytes(archive.read_bytes())
 print('Verified '+name,flush=True)
if __name__=='__main__':
 with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:list(pool.map(fetch,RUNTIMES))
