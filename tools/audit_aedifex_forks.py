#!/usr/bin/env python3
"""Audita checkouts locais de forks; não mescla nada automaticamente."""
from __future__ import annotations
import argparse,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
BASE=json.loads((ROOT/'integrations/aedifex/UPSTREAM_LOCK.json').read_text())['commit']
def run(args,cwd):
 c=subprocess.run(args,cwd=cwd,text=True,capture_output=True);return c.returncode,c.stdout.strip(),c.stderr.strip()
def audit(path):
 code,head,_=run(['git','rev-parse','HEAD'],path); license_path=path/'LICENSE'
 record={'path':str(path),'head':head if code==0 else None,'license_mit':license_path.is_file() and 'MIT License' in license_path.read_text(errors='ignore')}
 code,base,_=run(['git','merge-base',BASE,'HEAD'],path); record['shares_base']=code==0
 if code==0:
  _,stat,_=run(['git','diff','--stat',BASE+'..HEAD'],path);_,names,_=run(['git','diff','--name-only',BASE+'..HEAD'],path)
  record['diff_stat']=stat;record['changed_files']=names.splitlines();record['candidate']=record['license_mit'] and bool(record['changed_files'])
 else: record['candidate']=False;record['reason']='sem ancestral comum verificável'
 return record
def main():
 ap=argparse.ArgumentParser();ap.add_argument('paths',nargs='+',type=Path);ap.add_argument('--output',type=Path,default=ROOT/'validation/aedifex-forks.json');args=ap.parse_args()
 result={'base_commit':BASE,'forks':[audit(p.resolve()) for p in args.paths]};args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n');print(args.output)
if __name__=='__main__':main()
