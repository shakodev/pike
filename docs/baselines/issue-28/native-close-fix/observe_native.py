import pathlib, subprocess, time
root = pathlib.Path.cwd()
out = root / 'target/issue-28/fix'
with (out / 'original-observed-test.log').open('w') as log:
    proc = subprocess.Popen([str(root / 'target/debug/deps/pike-8047c988abb10012'), 'app::tests::closing::close_during_output_is_nonblocking_idempotent_and_waits_for_native_receipt', '--exact', '--nocapture', '--test-threads=1'], stdout=log, stderr=subprocess.STDOUT)
    time.sleep(1)
    snapshot = subprocess.check_output(['ps', '-axo', 'pid,ppid,pgid,sess,state,wchan,command'], text=True)
    rows = snapshot.splitlines()
    family = {proc.pid}
    for _ in range(3):
        for row in rows[1:]:
            fields = row.split(None, 6)
            if int(fields[1]) in family:
                family.add(int(fields[0]))
    selected = [rows[0]] + [row for row in rows[1:] if int(row.split()[0]) in family]
    (out / 'original-processes.txt').write_text('\n'.join(selected) + '\n')
    with (out / 'original-fds.txt').open('w') as fds:
        subprocess.run(['lsof', '-nP', '-a', '-p', ','.join(map(str, sorted(family)))], stdout=fds, stderr=subprocess.STDOUT)
    subprocess.run(['sample', str(proc.pid), '1', '-file', str(out / 'original-native.sample.txt')], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    result = proc.wait(timeout=20)
(out / 'original-exit.txt').write_text(f'exit={result}\n')
print((out / 'original-processes.txt').read_text())
print((out / 'original-fds.txt').read_text())
print((out / 'original-observed-test.log').read_text())
