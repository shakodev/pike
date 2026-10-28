import subprocess,sys,json
from pathlib import Path
label=sys.argv[1]
target=Path(sys.argv[2]).absolute() if len(sys.argv)>2 else Path.cwd()/'target/reviewer2'
deps=target/'debug/deps'
lib=max(deps.glob('libalacritty_terminal-*.rlib'),key=lambda p:p.stat().st_mtime)
poll=max(deps.glob('libpolling-*.rlib'),key=lambda p:p.stat().st_mtime)
exe=target/('sync-deadline-review-probe-'+label)
cmd=['rustc','--edition=2024','/private/tmp/pike-reviewer2-sync-deadline-probe.rs','-L','dependency='+str(deps),'--extern','alacritty_terminal='+str(lib),'--extern','polling='+str(poll),'-o',str(exe)]
print(json.dumps({'compile':cmd}),flush=True)
subprocess.run(cmd,check=True)
result=subprocess.run([str(exe)],capture_output=True,text=True,timeout=10)
print(result.stdout,end='');print(result.stderr,end='')
raise SystemExit(result.returncode)
