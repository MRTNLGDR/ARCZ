#!/usr/bin/env python3
from __future__ import annotations
import argparse, hashlib, json, subprocess, sys, tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "upstreams" / "manifest.toml"

def run(cmd, cwd=None, dry=False):
    printable = " ".join(map(str, cmd))
    print(f"+ {printable}")
    if dry: return ""
    proc = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)
    if proc.returncode:
        sys.stderr.write(proc.stdout + proc.stderr)
        raise SystemExit(proc.returncode)
    return proc.stdout.strip()

def sha256(path: Path) -> str:
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024), b''): h.update(chunk)
    return h.hexdigest()

def main():
    ap=argparse.ArgumentParser(description="Materialize exact ARCZ upstream snapshots without modifying them")
    ap.add_argument('--only', action='append', default=[])
    ap.add_argument('--dry-run', action='store_true')
    ap.add_argument('--reset', action='store_true', help='discard local changes in an upstream checkout')
    args=ap.parse_args()
    data=tomllib.loads(MANIFEST.read_text(encoding='utf-8'))
    selected=set(args.only)
    for src in data['source']:
        if selected and src['id'] not in selected: continue
        path=ROOT/src['path']
        if not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            run(['git','clone','--filter=blob:none','--no-checkout',src['repository'],str(path)],dry=args.dry_run)
        if args.dry_run: 
            print(f"  pin {src['id']} -> {src['commit']}")
            continue
        if args.reset:
            run(['git','reset','--hard'],cwd=path)
            run(['git','clean','-fdx'],cwd=path)
        dirty=run(['git','status','--porcelain'],cwd=path)
        if dirty:
            raise SystemExit(f"refusing dirty immutable upstream: {src['id']}\n{dirty}")
        run(['git','remote','set-url','origin',src['repository']],cwd=path)
        run(['git','fetch','--depth','1','origin',src['commit']],cwd=path)
        run(['git','checkout','--detach',src['commit']],cwd=path)
        head=run(['git','rev-parse','HEAD'],cwd=path)
        if head != src['commit']:
            raise SystemExit(f"pin mismatch for {src['id']}: {head}")
        candidates=['LICENSE','LICENSE.md','COPYING','COPYING.md']
        licenses=[]
        for name in candidates:
            candidate=path/name
            if candidate.exists(): licenses.append({'path':name,'sha256':sha256(candidate)})
        stamp={'schema_version':1,'id':src['id'],'repository':src['repository'],'commit':head,
               'declared_license':src['license'],'license_files':licenses,'immutable':True}
        (path/'.arcz-upstream.json').write_text(json.dumps(stamp,indent=2)+"\n",encoding='utf-8')
        print(f"OK {src['id']} {head}")

if __name__ == '__main__': main()
