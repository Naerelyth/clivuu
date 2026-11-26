"""Real old Selection input methods versus the shared pure Rust geometry."""
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile
ROOT=Path(__file__).resolve().parents[1]
env=os.environ.copy()
env.pop('APPIMAGE',None)
rng=random.Random(49382)
cases=[]
for i in range(500):
    scale=[1,1.25,1.5,2][i%4]
    outputs=[dict(x=-1280,y=0,width=1280,height=800,scale=scale),dict(x=0,y=-100,width=1920,height=1080,scale=2)]
    window=dict(x=-1000,y=100,width=600,height=400)
    preset=None if i%3==0 else dict(x=-900,y=150,width=550,height=350)
    actions=[dict(op='motion',point=[-700,250])]
    if preset is None:actions += [dict(op='scroll',delta=1),dict(op='scroll',delta=-1)]
    start=rng.choice([[-900,150],[-350,500],[-896.5,250],[-1250,780],[1800,850],[-700,250]])
    actions += [dict(op='begin',point=start)]
    for _ in range(10):
        actions += [dict(op='motion',point=rng.choice([[-1280+1/scale,0],[1919.5,979.5],[-.25,200],[0,200],[rng.uniform(-1280,1920),rng.uniform(-100,980)]]))]
    actions += [dict(op='end'),dict(op='nudge',point=[1,0]),dict(op='nudge',point=[0,-1],resize=True),dict(op='cycle',backwards=bool(i%2)),dict(op='full')]
    cases.append(dict(outputs=outputs,windows=[window],preset=preset,dimensions=[3840,1296],actions=actions))
def compare(a,b,path=''):
    if isinstance(a,dict):
        assert a.keys()==b.keys(),path
        for key in a:compare(a[key],b[key],path+'.'+key)
    elif isinstance(a,list):
        assert len(a)==len(b),path
        for i,(x,y) in enumerate(zip(a,b)):compare(x,y,path+f'[{i}]')
    else:assert abs(a-b)<1e-8 if isinstance(a,(int,float)) else a==b,(path,a,b)
with tempfile.TemporaryDirectory(prefix='clivuu-selection-parity-',dir='/tmp') as directory:
    work=Path(directory)
    (work/'cases.json').write_text(json.dumps(cases))
    subprocess.run(['gjs','-m',str(ROOT/'tests/selection-reference.js'),str(work/'cases.json'),str(work/'legacy.json')],env=env,cwd=ROOT,check=True,timeout=30)
    (work/'cases.jsonl').write_text(''.join(json.dumps(c)+'\n' for c in cases))
    with (work/'cases.jsonl').open() as inp,(work/'native.jsonl').open('w') as out:
        subprocess.run(['/tmp/clivuu-target/debug/examples/selection_probe'],env=env,stdin=inp,stdout=out,check=True,timeout=30)
    old=json.loads((work/'legacy.json').read_text())
    new=[json.loads(s) for s in (work/'native.jsonl').read_text().splitlines()]
    compare(old,new)
    print(f'PASS {len(cases)} capture input traces / {sum(len(c["actions"]) for c in cases)} states against real legacy methods')
