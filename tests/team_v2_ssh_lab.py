"""Loopback-only sshd. Uses an existing disposable lab OS account; never changes its password or keys."""
import json,pathlib,shutil,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
LAB=ROOT/'.lab/team-v2-http'
def main():
    subprocess.run(['id','termterm_test'],check=True,stdout=subprocess.DEVNULL)
    base=pathlib.Path('/var/lib/termterm-team-v2-ssh-lab');base.mkdir(mode=0o755,exist_ok=True)
    root=pathlib.Path(tempfile.mkdtemp(prefix='instance-',dir=base));root.chmod(0o755)
    proc=None;stop=LAB/'ssh.stop';stop.unlink(missing_ok=True)
    try:
        public=[]
        for name in ['rsa4096.pkcs1','rsa2048.pkcs1']:
            key=root/name;shutil.copyfile(ROOT/'tests/ssh-keys'/name,key);key.chmod(0o600)
            public.append(subprocess.check_output(['ssh-keygen','-y','-f',str(key)],text=True))
        authorized=root/'authorized_keys';authorized.write_text(''.join(public));authorized.chmod(0o644)
        host=root/'hostkey';subprocess.run(['ssh-keygen','-q','-t','ed25519','-N','','-f',str(host)],check=True)
        ca=root/'user-ca';subprocess.run(['ssh-keygen','-q','-t','ed25519','-N','','-f',str(ca)],check=True)
        user_public=root/'user.pub';user_public.write_text(public[0])
        subprocess.run(['ssh-keygen','-q','-s',str(ca),'-I','termterm-disposable','-n','termterm_test','-V','-5m:+1h',str(user_public)],check=True)
        user_certificate=(root/'user-cert.pub').read_text()
        config=root/'sshd_config';config.write_text(f'''Port 41843
ListenAddress 127.0.0.1
HostKey {host}
TrustedUserCAKeys {ca}.pub
PidFile {root}/sshd.pid
AuthorizedKeysFile {authorized}
AllowUsers termterm_test
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
PermitRootLogin no
AllowTcpForwarding yes
X11Forwarding no
UsePAM no
Subsystem sftp internal-sftp
LogLevel VERBOSE
''')
        subprocess.run(['/usr/sbin/sshd','-t','-f',str(config)],check=True)
        proc=subprocess.Popen(['/usr/sbin/sshd','-D','-e','-f',str(config)],stderr=(root/'server.log').open('w'))
        (LAB/'ssh-native.json').write_text(json.dumps({'host':'127.0.0.1','port':41843,'username':'termterm_test','certificate':user_certificate,'log':str(root/'server.log')}))
        print('Disposable SSH loopback fixture ready on 41843',flush=True)
        while not stop.exists():
            if proc.poll() is not None:raise RuntimeError('Disposable sshd stopped')
            time.sleep(.5)
    finally:
        if proc:
            proc.terminate()
            try:proc.wait(timeout=5)
            except subprocess.TimeoutExpired:proc.kill();proc.wait()
        (LAB/'ssh-native.json').unlink(missing_ok=True);stop.unlink(missing_ok=True)
        # Temporary path was created by tempfile and never derived from source data.
        if root.parent==base and root.name.startswith('instance-'):
            shutil.rmtree(root)
if __name__=='__main__':main()
