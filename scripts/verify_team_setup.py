#!/usr/bin/env python3
"""Require the PostgreSQL setup kit in every Team service delivery."""
import argparse
from pathlib import Path
import tarfile


REQUIRED_FILES = (
    'README.md',
    'docs/TEAM_SOCKET_KURULUM_TR.md',
    'docs/TEAM_DB_OWNER_TEMPLATE_TR.md',
    'docs/TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md',
    'docs/TEAM_SERVICE_INSTALL_TR.md',
    'docs/TEAM_UPGRADES_TR.md',
    'deploy/prepare-database-owner-template.sql',
    'deploy/prepare-database.sql',
    'deploy/service.socket.example.json',
    'deploy/service.local.example.json',
    'deploy/service.example.json',
    'deploy/deployment.env.example',
    'deploy/compose.yaml',
    'deploy/compose.socket.yaml',
    'deploy/compose.postgres.yaml',
    'deploy/Caddyfile',
    'deploy/Caddyfile.internal',
    'deploy/manage.py',
    'deploy/upgrade.py',
)


def verify_setup(package: Path):
    if package.is_dir():
        missing = [name for name in REQUIRED_FILES
                   if not (package / name).is_file() or (package / name).stat().st_size == 0]
    else:
        with tarfile.open(package, 'r:gz') as archive:
            members = {item.name: item for item in archive.getmembers()}
            missing = []
            for name in REQUIRED_FILES:
                item = members.get('team-service-linux-x64/' + name)
                if item is None or not item.isfile() or item.size == 0:
                    missing.append(name)
    if missing:
        raise ValueError('Missing or empty Team setup files: ' + ', '.join(missing))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('package', type=Path, help='Staged service directory or .tar.gz delivery')
    args = parser.parse_args()
    try:
        verify_setup(args.package)
    except (OSError, ValueError, tarfile.TarError) as error:
        parser.exit(1, str(error) + '\n')
    print('PASS: PostgreSQL SQL, config examples and step-by-step setup guides are included.')
