#!/usr/bin/env python3
"""Restore pinned third-party archives from the retained v0.4.3 source release."""
import argparse
import hashlib
from pathlib import Path
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SOURCE_URL = 'https://github.com/belalhushelmy2-cmyk/SNI-Tunnel/releases/download/v0.4.3/sni-tunnel-source.zip'
SOURCE_SHA = '7edebc06b2ed9fe67b7d619f0d410056b195751bcf2961123700673d9af55220'
ARCHIVES = {
    'Xray-linux-64.zip': 'fa6517eea07f10f5f0a7e8768bb4b411266a217161b0a9d2794b48769f1ad541',
    'badvpn-source.tar.gz': '04a841b52c66d3ebd3ce12f005608c53a7cef751457914bf39ac8e5d6bab3775',
    'Xray-1.6.1-source.tar.gz': '46d83fa77c1c7873eefc55c16c337b32fd58cead846e241b0f2698fc7f380a18',
}

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--directory', type=Path, default=ROOT/'packaging/vendor')
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    missing = []
    for name, expected in ARCHIVES.items():
        target = args.directory/name
        if target.exists():
            if sha(target) != expected:
                raise RuntimeError(f'Existing {name} checksum mismatch; refusing replacement')
        else:
            missing.append(name)
    if not missing:
        print('Vendor archives verified')
        return
    cache = ROOT/'target/vendor-source-v0.4.3.zip'
    cache.parent.mkdir(parents=True, exist_ok=True)
    if not cache.exists():
        temporary = cache.with_suffix('.download')
        urllib.request.urlretrieve(SOURCE_URL, temporary)
        if sha(temporary) != SOURCE_SHA:
            temporary.unlink()
            raise RuntimeError('Previous source archive checksum mismatch')
        temporary.rename(cache)
    if sha(cache) != SOURCE_SHA:
        raise RuntimeError('Cached previous source archive checksum mismatch')
    with zipfile.ZipFile(cache) as archive:
        for name in missing:
            matches = [n for n in archive.namelist() if n.endswith('/packaging/vendor/'+name)]
            if len(matches) != 1:
                raise RuntimeError(f'Cannot uniquely locate {name}')
            raw = archive.read(matches[0])
            if hashlib.sha256(raw).hexdigest() != ARCHIVES[name]:
                raise RuntimeError(f'{name} checksum mismatch')
            temporary = args.directory/(name+'.download')
            temporary.write_bytes(raw)
            temporary.replace(args.directory/name)
    print('Restored and verified', len(missing), 'vendor archives')

if __name__ == '__main__':
    main()
