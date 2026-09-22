"""Regenerate disposable OpenSSL interoperability keys. NEVER use them on a server."""
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1] / 'tests' / 'ssh-keys'
OPENSSL = os.environ.get('OPENSSL', shutil.which('openssl') or '')
PASS = 'termterm-disposable-test'

def run(*args):
    subprocess.run([OPENSSL, *map(str, args)], check=True, capture_output=True)

ROOT.mkdir(parents=True, exist_ok=True)
for bits in (2048, 3072, 4096):
    key = ROOT / f'rsa{bits}.pkcs8'
    run('genpkey', '-algorithm', 'RSA', '-pkeyopt', f'rsa_keygen_bits:{bits}', '-out', key)
    run('rsa', '-in', key, '-traditional', '-out', ROOT / f'rsa{bits}.pkcs1')
    if bits == 4096:
        for cipher in ('aes128', 'aes192', 'aes256'):
            run('rsa', '-in', key, '-traditional', f'-{cipher}', '-passout', f'pass:{PASS}', '-out', ROOT / f'rsa4096.{cipher}.pem')
        run('pkcs8', '-topk8', '-in', key, '-v2', 'aes-256-cbc', '-passout', f'pass:{PASS}', '-out', ROOT / 'rsa4096.encrypted.pkcs8')
for bits, curve in ((256, 'prime256v1'), (384, 'secp384r1'), (521, 'secp521r1')):
    key = ROOT / f'ecdsa{bits}.sec1'
    run('ecparam', '-name', curve, '-genkey', '-noout', '-out', key)
    run('pkcs8', '-topk8', '-nocrypt', '-in', key, '-out', ROOT / f'ecdsa{bits}.pkcs8')
    run('ec', '-in', key, '-aes256', '-passout', f'pass:{PASS}', '-out', ROOT / f'ecdsa{bits}.aes256.pem')
run('genpkey', '-algorithm', 'ED25519', '-out', ROOT / 'ed25519.pkcs8')
run('pkcs8', '-topk8', '-in', ROOT / 'ed25519.pkcs8', '-v2', 'aes-256-cbc', '-passout', f'pass:{PASS}', '-out', ROOT / 'ed25519.encrypted.pkcs8')
(ROOT / 'generator.json').write_text(json.dumps({'generator': subprocess.check_output([OPENSSL, 'version'], text=True).strip(), 'passphrase': PASS, 'warning': 'Disposable public test keys, never authorize on a real server'}, indent=2) + '\n', encoding='utf-8')
print('Generated disposable OpenSSL interoperability fixtures')
