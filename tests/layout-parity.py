#!/usr/bin/env python3
"""Run independent geometry implementations without synchronous Node children."""
import os
from pathlib import Path
import subprocess
import tempfile
ROOT=Path(__file__).resolve().parents[1]
env=os.environ.copy()
env.pop('APPIMAGE',None)
with tempfile.TemporaryDirectory(prefix='clivuu-layout-parity-',dir='/tmp') as work:
    source=Path(work)/'cases.jsonl'
    result=Path(work)/'rust.jsonl'
    with source.open('w') as out:
        subprocess.run(['node','tests/layout-parity.mjs','--emit'],cwd=ROOT,env=env,stdout=out,check=True,timeout=20)
    with source.open() as inp,result.open('w') as out:
        subprocess.run(['/tmp/clivuu-target/debug/examples/layout_probe'],stdin=inp,stdout=out,env=env,check=True,timeout=30)
    subprocess.run(['node','tests/layout-parity.mjs',str(result)],cwd=ROOT,env=env,check=True,timeout=30)
