import csv,hashlib,json,os,subprocess,sys,threading,time
from pathlib import Path
exe=Path('/private/tmp/r38-resize-builds/tiny-skia-baseline/renderer_comparison')
root=Path(sys.argv[1]); root.mkdir()
cmd=[str(exe),'--output',str(root/'fixture'),'--visible','8','--hidden','0','--history','empty','--workload','flood','--seconds','3','--settle','0','--probes','0','--synthetic-focus','true']
env=os.environ.copy();env.update(RUST_LOG='off',ICED_BACKEND='tiny-skia')
p=subprocess.Popen(cmd,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,start_new_session=True)
err=bytearray()
def drain():
 for b in iter(lambda:p.stderr.read(4096),b''): err.extend(b[:max(0,4096-len(err))])
t=threading.Thread(target=drain,daemon=True);t.start()
sample=None; children=set(); started=time.monotonic()
try:
 while p.poll() is None and time.monotonic()-started<40:
  events=root/'fixture/events.csv'
  rows=list(csv.DictReader(events.open())) if events.exists() else []
  children.update(int(r['a']) for r in rows if r['event']=='ready')
  if sample is None and any(r['event']=='measure_begin' for r in rows):
   sample=subprocess.Popen(['/usr/bin/sample',str(p.pid),'2','5','-file',str(root/'sample.txt')],stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  time.sleep(.02)
finally:
 if p.poll() is None:
  p.terminate()
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:p.kill();p.wait(timeout=5)
 if sample:
  try: sample_log=sample.communicate(timeout=5)[0]
  except subprocess.TimeoutExpired:sample.kill();sample_log=sample.communicate(timeout=5)[0]
  (root/'sample-command.txt').write_bytes(sample_log)
 t.join(5)
 (root/'stderr.txt').write_bytes(err)
 observed=subprocess.run(['/bin/ps','-p',','.join(map(str,sorted(children))),'-o','pid=,ppid=,lstart=,stat=,comm='],capture_output=True,text=True) if children else None
 (root/'identity.json').write_text(json.dumps(dict(command=cmd,sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),app_pid=p.pid,children=sorted(children),survivors=observed.stdout.splitlines() if observed else [],stderr_finished=not t.is_alive(),exit_code=p.returncode,wall_seconds=time.monotonic()-started,scope='sampling diagnostic; observer overhead excludes this from cost evidence'),indent=2)+'\n')
print((root/'identity.json').read_text())
