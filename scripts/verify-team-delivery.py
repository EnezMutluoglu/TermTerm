#!/usr/bin/env python3
"""Validate local development delivery names, archive hygiene and SHA256 sums."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import zipfile

root = Path(__file__).resolve().parents[1]
version = json.loads((root/'package.json').read_text())['version']
assert '-dev.' in version
out = root/'artifacts'/('team-'+version)
# A package can be intact yet stale. Never rewrite its successful delivery
# manifest against changed source, or confuse archive integrity with readiness.
fingerprint = json.loads((out/'SOURCE_FINGERPRINT.json').read_text(encoding='utf-8-sig'))
assert fingerprint['version'] == version, 'Source fingerprint version differs'
seen = set()
changed = []
for entry in fingerprint['files']:
    relative = PurePosixPath(entry['file'])
    assert not relative.is_absolute() and '..' not in relative.parts and ':' not in str(relative), 'Unsafe source fingerprint path'
    assert str(relative) not in seen, 'Duplicate source fingerprint entry'
    seen.add(str(relative))
    source = root/relative
    if not source.is_file():
        changed.append(str(relative))
    else:
        with source.open('rb') as stream: actual = hashlib.file_digest(stream,'sha256').hexdigest()
        if actual != entry['sha256']: changed.append(str(relative))
assert not changed, 'Packages are stale; rebuild from current source before delivery: '+', '.join(changed[:8])
assert {'team-service/compatibility.json', 'team-service/deploy/upgrade.py',
        'team-service/src/compatibility.rs', 'team-client/src/compatibility.rs', 'src/TeamCompatibility.tsx', 'src/TeamUpdatePolicy.tsx', 'team-service/migrations/062_client_update_policy.sql', 'team-service/migrations/063_route_snapshots.sql',
        'team-service/migrations/064_proxy_restore.sql', 'team-client/src/proxy.rs', 'src/TeamProxy.tsx',
        'src/TeamIdentities.tsx', 'src/TeamTerminals.tsx', 'src/inputQueue.ts', 'src-tauri/src/team_v2/signin.rs'} <= seen, 'Source fingerprint omits current upgrade/migration components; rebuild packages'
required = [out/f'TermTerm Team Dev_{version}_x64-setup.exe',
            out/f'TermTerm-Team-{version}-windows-x64-portable.zip',
            out/'linux-x64'/f'TermTerm Team Dev_{version}_amd64.deb',
            out/'linux-x64'/f'TermTerm Team Dev_{version}_amd64.AppImage',
            out/f'TermTerm-Team-Service-{version}-linux-x64.tar.gz']
text_extensions = {'.json','.md','.txt','.sql','.sh','.py','.yaml','.yml','.toml','.conf'}


def inspect(name, data=None):
    name = name.replace('\\','/')
    p = PurePosixPath(name)
    assert not p.is_absolute() and '..' not in p.parts and not re.match(r'^[A-Za-z]:', name), 'Unsafe archive path'
    assert not set(p.parts) & {'.lab','.tools','.git','node_modules','target','tests','__pycache__'}, 'Private/build path packaged'
    assert not p.suffix in {'.ttvault','.ttbackup','.ttcache','.ttrecovery','.ttmigration','.ttlogin','.ttpolicy','.ttsettings','.dump','.pgpass'}, 'Private state packaged'
    assert p.name not in {'service.json','database.json','windows-native.json','deployment.env','migration.pgpass'}, 'Private deployment data packaged'
    if data is not None and p.suffix in text_extensions:
        assert not re.search(rb'-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----',data), 'Private key material packaged'
        assert b'__nativeEvents' not in data and b'__E2E__' not in data, 'Test hook packaged'


for path in required:
    assert path.is_file() and path.stat().st_size > 10000, 'Missing or truncated package: '+path.name
    if path.suffix == '.zip':
        with zipfile.ZipFile(path) as archive:
            assert archive.testzip() is None
            for item in archive.infolist():
                inspect(item.filename,archive.read(item) if not item.is_dir() and PurePosixPath(item.filename).suffix in text_extensions else None)
    elif path.name.endswith('.tar.gz'):
        with tarfile.open(path) as archive:
            contract = json.load(archive.extractfile('team-service-linux-x64/compatibility.json'))
            assert contract == json.loads((root/'team-service/compatibility.json').read_text()), 'Packaged service schema/API contract is stale'
            for item in archive:
                inspect(item.name,archive.extractfile(item).read() if item.isfile() and PurePosixPath(item.name).suffix in text_extensions else None)
                if item.issym() or item.islnk():inspect(item.linkname)
hashes=[]
for path in required:
    with path.open('rb') as f: digest=hashlib.file_digest(f,'sha256').hexdigest()
    hashes.append(digest+'  '+path.relative_to(out).as_posix())
(out/'SHA256SUMS.txt').write_text('\n'.join(hashes)+'\n',encoding='utf-8')
(out/'DELIVERY.json').write_text(json.dumps({'version':version,'channel':'development','stablePublished':False,
    'completeFeatureScope':False,'packages':[{'file':p.relative_to(out).as_posix(),'bytes':p.stat().st_size} for p in required],
    'limitations':['Windows agent/Pageant and production OTP providers not qualified',
        'Combined Windows native acceptance: 22/22; see report for earlier failed attempts',
        'macOS and FIDO2/Windows Hello deferred by owner'],
    'report':'TEAM_041_STATUS.md'},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('PASS: five local development packages, ZIP integrity, archive hygiene and SHA256 manifest. Feature scope remains incomplete; no release published.')
