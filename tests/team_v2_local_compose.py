"""Validate the published image and socket Compose configuration without production state."""
import gzip, hashlib, json, os, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
version=json.loads((ROOT/'package.json').read_text())['version']
out=ROOT/'artifacts'/('release-'+version)
manifest=json.loads((out/f'TermTerm-Team-Image-{version}-linux-x64.json').read_text())
archive=out/manifest['archive']['file']
assert hashlib.sha256(archive.read_bytes()).hexdigest()==manifest['archive']['sha256']
with gzip.open(archive,'rb') as image:
    subprocess.run(['docker','load'],input=image.read(),check=True,capture_output=True)
info=json.loads(subprocess.check_output(['docker','image','inspect',manifest['image']]))[0]
assert info['Id']==manifest['imageId'] and info['Config']['User']=='10001:10001'
deploy=out/'team-service-linux-x64/deploy'
env={**os.environ,'TERMTERM_CONFIG_DIR':'/etc/termterm-team-private/config',
     'TERMTERM_DOMAIN':'10.20.30.10','TERMTERM_SERVICE_IMAGE':manifest['image'],
     'TERMTERM_CADDY_FILE':str(deploy/'Caddyfile.internal')}
config=json.loads(subprocess.check_output(['docker','compose','-f',str(deploy/'compose.yaml'),
    '-f',str(deploy/'compose.socket.yaml'),'config','--format','json'],env=env))
team=config['services']['team']
assert team['network_mode']=='host' and team['read_only'] and team['restart']=='unless-stopped'
assert float(team['cpus'])==1 and int(team['mem_limit'])==512*1024*1024 and int(team['pids_limit'])==128
assert team['cap_drop']==['ALL'] and any('no-new-privileges' in s for s in team['security_opt'])
assert any(v['source']=='/var/run/postgresql' and v['target']=='/var/run/postgresql' and v['read_only'] for v in team['volumes'])
assert not team.get('ports')
binary=subprocess.check_output(['docker','run','--rm','--entrypoint','sha256sum',manifest['image'],'/usr/local/bin/termterm-team-service']).decode().split()[0]
assert binary==manifest['binarySha256']
os_info=subprocess.check_output(['docker','run','--rm','--entrypoint','cat',manifest['image'],'/etc/os-release']).decode()
assert 'VERSION_ID="13"' in os_info
print('PASS: image archive/load/identity/binary, Debian 13 nonroot, socket mount, host network, read-only, CPU/RAM/pids and restart configuration')
