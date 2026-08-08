#!/usr/bin/env python3
from __future__ import annotations
import argparse, hashlib, json
from pathlib import Path
import subprocess, sys


def sha(path: Path) -> str:
    d=hashlib.sha256()
    with path.open('rb') as f:
        for c in iter(lambda:f.read(1024*1024),b''): d.update(c)
    return d.hexdigest()


def main() -> int:
    p=argparse.ArgumentParser(); p.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2]); a=p.parse_args()
    root=a.root.resolve(); lock=json.loads((root/'integrations/aedifex/upstream.lock.json').read_text())
    vendor=(root/'integrations/aedifex/vendor').resolve(); problems=[]
    if not vendor.is_dir(): problems.append('vendor missing')
    else:
        if (vendor/'.git').is_dir():
            head=subprocess.run(['git','rev-parse','HEAD'],cwd=vendor,text=True,capture_output=True).stdout.strip()
            if head!=lock['primary']['commit']: problems.append(f'commit mismatch: {head}')
        else: problems.append('vendor has no Git metadata')
        if not (vendor/'apps/arcz-host/package.json').is_file(): problems.append('ARCZ overlay missing')
        if not (vendor/'LICENSE').is_file() or 'MIT License' not in (vendor/'LICENSE').read_text(errors='replace'):
            problems.append('MIT license missing')
    dist=root/'integrations/aedifex/dist'; dist_manifest=dist/'manifest.json'
    if dist_manifest.is_file():
        manifest=json.loads(dist_manifest.read_text())
        for item in manifest.get('files',[]):
            path=dist/item['path']
            if not path.is_file() or path.stat().st_size!=item['bytes'] or sha(path)!=item['sha256']:
                problems.append(f'dist artifact invalid: {item["path"]}')
    print(json.dumps({'ok':not problems,'problems':problems,'commit':lock['primary']['commit']},indent=2))
    return 0 if not problems else 2

if __name__=='__main__':
    raise SystemExit(main())
