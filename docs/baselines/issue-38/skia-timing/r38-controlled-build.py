import hashlib,json,os,subprocess,sys
from pathlib import Path
repo=Path('/private/tmp/pike-issue-38'); os.chdir(repo)
output=Path(sys.argv[1]); before='--before' in sys.argv[2:]
files=['vendor/iced_term/src/view.rs','vendor/iced_term/src/view/glyphs.rs','vendor/iced_term/src/view/glyphs/tests.rs']
saved={p:Path(p).read_bytes() for p in files}
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',PYTHONDONTWRITEBYTECODE='1')
try:
 if before:
  for p in files:Path(p).write_bytes(subprocess.check_output(['git','show','HEAD:'+p]))
 command=['python3','tools/renderer-comparison.py','build','--output',str(output),'--target-directory','/Users/daan.hoedjes/Documents/Projects/pike/target','--renderers','tiny-skia','--variants','baseline']
 with output.with_suffix('.log').open('w') as log:subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 build=json.loads((output/'builds.json').read_text())[0]
 invocation=build['invocation'];i=invocation.index('--bin');invocation=invocation[:i]+['--example','unicode_cells']+invocation[i+4:]
 (output/'unicode-build-command.json').write_text(json.dumps(invocation,indent=2)+'\n')
 with (output/'unicode-build.log').open('w') as log:subprocess.run(invocation,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 import shutil
 origin=Path('/Users/daan.hoedjes/Documents/Projects/pike/target/aarch64-apple-darwin/release/examples/unicode_cells')
 shutil.copy2(origin,output/'tiny-skia-baseline/unicode_cells')
 (output/'unicode-sha256.txt').write_text(hashlib.sha256(origin.read_bytes()).hexdigest()+'\n')
finally:
 if before:
  for p,data in saved.items():Path(p).write_bytes(data)
print('built',output)
