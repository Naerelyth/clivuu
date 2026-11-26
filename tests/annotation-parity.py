#!/usr/bin/env python3
"""Run independent JS/Rust models against identical command traces in /tmp."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]
env=os.environ.copy()
env.pop('APPIMAGE',None)
subprocess.run(['cargo','build','--offline','-p','clivuu-core','--example','annotation_probe'],cwd=ROOT,env=env,check=True)
with tempfile.TemporaryDirectory(prefix='clivuu-annotation-parity-',dir='/tmp') as work:
    source=Path(work)/'commands.jsonl'
    result=Path(work)/'rust.jsonl'
    with source.open('w') as out:
        subprocess.run(['node','tests/annotation-parity.mjs','--emit'],cwd=ROOT,env=env,stdout=out,check=True,timeout=20)
    with source.open() as inp,result.open('w') as out:
        subprocess.run(['/tmp/clivuu-target/debug/examples/annotation_probe'],stdin=inp,stdout=out,env=env,check=True,timeout=30)
    subprocess.run(['node','tests/annotation-parity.mjs',str(result)],cwd=ROOT,env=env,check=True,timeout=30)
