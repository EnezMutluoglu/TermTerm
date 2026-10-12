#!/usr/bin/env python3
"""Build/save a versioned Linux image from a verified service delivery; no upload."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--stage', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--version', required=True)
p.add_argument('--revision', required=True, help='Source commit, with -dirty when applicable')
a = p.parse_args()
if not re.fullmatch(r'\d+\.\d+\.\d+(?:-dev\.\d+)?', a.version):
    p.error('Invalid service version')
binary = a.stage/'bin/termterm-team-service'
if not binary.is_file() or binary.read_bytes()[:4] != b'\x7fELF':
    p.error('Stage must contain the verified Linux executable')
a.output.mkdir(parents=True, exist_ok=True)
name = f'TermTerm-Team-Image-{a.version}-linux-x64'
archive = a.output/(name+'.tar.gz')
manifest = a.output/(name+'.json')
partial = archive.with_suffix('.partial')
if any(path.exists() for path in [archive, manifest, partial]):
    p.error('Image delivery already exists; do not overwrite a versioned archive')
image = 'termterm/team-service:'+a.version
subprocess.run(['docker', 'build', '--platform', 'linux/amd64',
                '--build-arg', 'VERSION='+a.version, '--build-arg', 'REVISION='+a.revision,
                '-f', str(a.stage/'deploy/Dockerfile'), '-t', image, str(a.stage)], check=True)
info = json.loads(subprocess.check_output(['docker', 'image', 'inspect', image]))[0]
assert info['Architecture'] == 'amd64' and info['Os'] == 'linux'
assert info['Config']['User'] == '10001:10001'
try:
    with partial.open('xb') as output:
        with gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=0) as zipped:
            process = subprocess.Popen(['docker', 'image', 'save', image], stdout=subprocess.PIPE)
            try:
                shutil.copyfileobj(process.stdout, zipped)
            finally:
                process.stdout.close()
                code = process.wait()
            if code:
                raise RuntimeError('docker image save failed')
    with partial.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    archive_hash = {'file': archive.name, 'sha256': digest, 'bytes': partial.stat().st_size}
    data = {'version': a.version, 'sourceRevision': a.revision, 'image': image,
            'imageId': info['Id'], 'uncompressedBytes': info['Size'], 'platform': 'linux/amd64',
            'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'archive': archive_hash}
    with manifest.open('x') as stream:
        json.dump(data, stream, indent=2); stream.write('\n')
    partial.rename(archive)
except BaseException:
    partial.unlink(missing_ok=True)
    raise
print(json.dumps(data, indent=2))
