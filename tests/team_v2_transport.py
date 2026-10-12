"""Operator tools share the service's explicit loopback-only plaintext policy."""
import importlib.util
import os
from pathlib import Path
import tempfile
from unittest.mock import patch

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('manage', root/'team-service/deploy/manage.py')
manage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manage)
with tempfile.TemporaryDirectory() as tmp:
    password_file = Path(tmp)/'pgpass'
    password_file.write_text('unused')
    password_file.chmod(0o600)
    env = dict(PGHOST='127.0.0.1', PGDATABASE='test', PGUSER='owner',
               PGPASSFILE=str(password_file), PGSSLMODE='disable')
    with patch.dict(os.environ, env, clear=True):
        assert manage.environment()['PGSSLMODE'] == 'disable'
    socket_env={k:v for k,v in env.items() if k!='PGPASSFILE'}
    socket_env['PGHOST']='/var/run/postgresql'
    with patch.dict(os.environ, socket_env, clear=True):
        assert 'PGPASSFILE' not in manage.environment()
    with patch.dict(os.environ, {**socket_env, 'PGHOSTADDR':'127.0.0.1'}, clear=True):
        try: manage.environment()
        except ValueError: pass
        else: raise AssertionError('Socket endpoint override accepted')
    for changes in [dict(PGHOST='localhost'), dict(PGHOST='10.1.2.3'),
                    dict(PGHOSTADDR='10.1.2.3'), dict(PGHOST='127.0.0.1,10.1.2.3'),
                    dict(PGSSLMODE='prefer'), dict(PGSSLMODE='require'),
                    dict(PGSSLMODE='verify-full')]:
        with patch.dict(os.environ, {**env, **changes}, clear=True):
            try:
                manage.environment()
            except ValueError:
                pass
            else:
                raise AssertionError('Insecure/incomplete transport was accepted')
    ca=Path(tmp)/'ca';ca.write_text('readability only, TLS verifies contents on connection')
    with patch.dict(os.environ, {**env, 'PGSSLMODE':'verify-full', 'PGSSLROOTCERT':str(ca)}, clear=True):
        assert manage.environment()['PGSSLMODE']=='verify-full'
print('PASS: explicit loopback mode, CA requirement, hostname/address overrides and downgrade rejection')
