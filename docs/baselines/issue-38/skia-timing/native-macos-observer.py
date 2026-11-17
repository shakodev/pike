"""Finite macOS synthetic diagnosis. RSS is not Windows private bytes/working set."""
import csv,json,os,subprocess,sys,threading,time
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'tools'))
from renderer_comparison_support import digest,read_csv,validate_fixture,write_json,distribution
manifest=Path(sys.argv[1]); root=Path(sys.argv[2]); root.mkdir()
plan=json.loads(manifest.read_text()); write_json(root/'run-manifest.json',plan)
write_json(root/'scope.json',dict(platform=sys.platform,scope='native macOS, synthetic focus, Unix PTY; diagnostic app RSS/cumulative CPU only',windows_private_bytes=None,windows_working_set=None,gpu=None,method='external ps samples at 250 ms; CPU is one-core delta over observed sample interval, RSS is resident KiB converted to bytes; observers run sequentially'))
assert 0<len(plan['runs'])<=64
outcomes=[]
for index,run in enumerate(plan['runs']):
 directory=root/f'run-{index:04d}'; directory.mkdir();write_json(directory/'run.json',run)
 exe=Path(run['build']['executable']);assert digest(exe)==run['build']['fixture_sha256']
 cmd=[str(exe),'--output',str(directory/'fixture'),'--visible',str(run['visible']),'--hidden',str(run['hidden']),'--history',run['history'],'--workload',run['workload'],'--seconds',str(run['seconds']),'--settle',str(run['settle']),'--probes',str(run['probes']),'--synthetic-focus','true']
 env=os.environ.copy();env.update(RUST_LOG='off',ICED_BACKEND=run['build']['renderer'])
 p=subprocess.Popen(cmd,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,start_new_session=True)
 prefix=bytearray()
 def drain():
  for data in iter(lambda:p.stderr.read(4096),b''):prefix.extend(data[:max(0,4096-len(prefix))])
 reader=threading.Thread(target=drain,daemon=True);reader.start()
 started=time.monotonic(); children=set(); samples=[]; error=None; success=False; next_sample=0; end_sampled=False
 try:
  while p.poll() is None and time.monotonic()-started<run['seconds']+run['settle']+40:
   event_path=directory/'fixture/events.csv'; events=read_csv(event_path) if event_path.exists() else []
   children.update(int(r['a']) for r in events if r['event']=='ready')
   began=any(r['event']=='measure_begin' for r in events); ended=any(r['event']=='measure_end' for r in events)
   if began and (time.monotonic()>=next_sample or (ended and not end_sampled)) and not end_sampled:
    before=time.monotonic()-started
    observed=subprocess.run(['/bin/ps','-p',str(p.pid),'-o','rss=,time='],capture_output=True,text=True,timeout=2)
    after=time.monotonic()-started
    fields=observed.stdout.split()
    if observed.returncode==0 and len(fields)==2:
     minutes,seconds=fields[1].split(':');cpu=int(minutes)*60+float(seconds)
     samples.append(dict(seconds=(before+after)/2,observer_ms=(after-before)*1000,phase='end' if ended else 'measure',rss_bytes=int(fields[0])*1024,cpu_seconds=cpu))
    else: raise ValueError('app resource observation unavailable')
    next_sample=time.monotonic()+.25
    end_sampled=ended
   time.sleep(.02)
  if p.poll() is None:raise ValueError('external diagnostic deadline')
  if p.returncode!=0:raise ValueError(f'fixture exited {p.returncode}')
  summary=validate_fixture(directory/'fixture',run['visible'],run['hidden'],run['history']=='saturated',run['workload']=='flood',run['probes'],run['seconds'])
  write_json(directory/'fixture-summary.json',summary);success=True
 except Exception as failure:error=str(failure)
 finally:
  if p.poll() is None:
   p.terminate()
   try:p.wait(timeout=5)
   except subprocess.TimeoutExpired:p.kill();p.wait(timeout=5)
  reader.join(5);p.stderr.close();(directory/'stderr.txt').write_bytes(prefix)
  absent=subprocess.run(['/bin/ps','-p',','.join(map(str,sorted(children))),'-o','pid=,ppid=,lstart=,stat=,comm='],capture_output=True,text=True) if children else None
  survivors=absent.stdout.splitlines() if absent else []
  if survivors or reader.is_alive(): success=False;error=error or 'cleanup not established'
  write_json(directory/'collector.json',dict(success=success,failure=error,app_pid=p.pid,ready_child_pids=sorted(children),child_processes_observed_after_app_exit=survivors,stderr_reader_finished=not reader.is_alive(),exit_code=p.returncode,external_wall_seconds=time.monotonic()-started,scope='macOS diagnostic only'))
  write_json(directory/'app-resource-samples.json',samples)
  if len(samples)>=2:
   a,b=samples[0],samples[-1]
   write_json(directory/'app-resource-summary.json',dict(sample_count=len(samples),observed_seconds=b['seconds']-a['seconds'],one_core_cpu_percent=(b['cpu_seconds']-a['cpu_seconds'])/(b['seconds']-a['seconds'])*100,rss_bytes=distribution(s['rss_bytes'] for s in samples),observer_ms=distribution(s['observer_ms'] for s in samples),scope='diagnostic app-only samples; Windows resource costs and physical input/display latency unavailable'))
 outcomes.append(dict(index=index,success=success,failure=error));write_json(root/'outcomes.json',outcomes)
 print(index,run['build']['source_commit'][:8],run['visible'],run['workload'],success,error,flush=True)
sys.exit(0 if all(r['success'] for r in outcomes) else 1)
