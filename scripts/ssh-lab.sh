#!/usr/bin/env bash
set -euo pipefail
out=${1:?Output directory required}
root=/var/lib/termterm-ssh
# The default daemon is newly installed by this project; keep only the isolated loopback test service.
systemctl disable --now ssh.socket ssh.service >/dev/null 2>&1 || true
mkdir -p "$root" "$out" /run/sshd
if ! id termterm_test >/dev/null 2>&1; then useradd -m -d "$root/home" -s /bin/bash termterm_test; fi
if [ ! -f "$root/password" ]; then openssl rand -base64 32 > "$root/password"; chmod 600 "$root/password"; fi
password=$(cat "$root/password")
printf 'termterm_test:%s\n' "$password" | chpasswd
if [ ! -f "$root/ssh_host_ed25519_key" ]; then ssh-keygen -q -t ed25519 -N '' -f "$root/ssh_host_ed25519_key"; fi
if [ ! -f "$root/client_key" ]; then ssh-keygen -q -t ed25519 -N '' -C termterm-lab -f "$root/client_key"; fi
install -d -m 700 -o termterm_test -g termterm_test "$root/home/.ssh"
install -m 600 -o termterm_test -g termterm_test "$root/client_key.pub" "$root/home/.ssh/authorized_keys"
cat > "$root/sshd_config" <<'EOF'
Port 22222
ListenAddress 127.0.0.1
HostKey /var/lib/termterm-ssh/ssh_host_ed25519_key
PidFile /var/lib/termterm-ssh/sshd.pid
AllowUsers termterm_test
PasswordAuthentication yes
PubkeyAuthentication yes
PermitRootLogin no
UsePAM no
AllowTcpForwarding yes
GatewayPorts no
PermitTTY yes
Subsystem sftp internal-sftp
EOF
cat > /etc/systemd/system/termterm-ssh.service <<'EOF'
[Unit]
Description=TermTerm isolated SSH test server
After=network.target
[Service]
RuntimeDirectory=sshd
RuntimeDirectoryMode=0755
ExecStart=/usr/sbin/sshd -D -e -f /var/lib/termterm-ssh/sshd_config
Restart=on-failure
[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now termterm-ssh.service >/dev/null
python3 - "$root" "$out" <<'PY'
import json,pathlib,sys
root,out=map(pathlib.Path,sys.argv[1:])
(out/'ssh.json').write_text(json.dumps({'address':'127.0.0.1','port':22222,'username':'termterm_test','password':(root/'password').read_text().strip(),'privateKey':(root/'client_key').read_text(),'publicKey':(root/'ssh_host_ed25519_key.pub').read_text().strip(),'home':str(root/'home')},indent=2))
(root/'home'/'welcome.txt').write_text('TermTerm SFTP integration fixture\nUnicode: İstanbul — 東京\n')
PY
chown termterm_test:termterm_test "$root/home/welcome.txt"
echo 'TermTerm SSH/SFTP test server ready on localhost:22222.'
