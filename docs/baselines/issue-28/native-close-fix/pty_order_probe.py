import fcntl, os, pathlib, signal, subprocess, termios, time
master, slave = os.openpty()
def child_setup():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_DFL)
child = subprocess.Popen(['/bin/sh', '-c', "while :; do printf 'PIKE28_FLOOD\\r\\n'; done"], stdin=slave, stdout=slave, stderr=slave, preexec_fn=child_setup)
os.close(slave)
first = os.read(master, 4096)
assert b'PIKE28_FLOOD' in first
# Let the controlled flood fill the slave's output queue, then follow Pty::drop.
time.sleep(0.1)
began = time.monotonic()
os.kill(child.pid, signal.SIGHUP)
try:
    child.wait(timeout=1)
    print('wait_before_master_close=completed', flush=True)
except subprocess.TimeoutExpired:
    print('wait_before_master_close=pending', flush=True)
    print(subprocess.check_output(['ps','-p', str(child.pid), '-o', 'pid,ppid,pgid,state,wchan,command'], text=True), flush=True)
os.close(master)
code = child.wait(timeout=3)
print(f'wait_after_master_close=completed pid={child.pid} code={code} elapsed_ms={(time.monotonic()-began)*1000:.1f}', flush=True)
