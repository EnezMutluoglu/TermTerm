"""No database or container required: exercise the delivery gate with small fixtures."""
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from verify_team_setup import REQUIRED_FILES, verify_setup


class SetupPackageTests(unittest.TestCase):
    def test_required_files_in_directory_and_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / 'stage'
            for name in REQUIRED_FILES:
                file = stage / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text('disposable fixture', encoding='utf-8')
            verify_setup(stage)
            archive = root / 'service.tar.gz'

            def check_archive():
                with tarfile.open(archive, 'w:gz') as target:
                    target.add(stage, arcname='team-service-linux-x64')
                verify_setup(archive)

            check_archive()
            for name in REQUIRED_FILES:
                with self.subTest(file=name):
                    file = stage / name
                    original = file.read_bytes()
                    file.unlink()
                    for check in (lambda: verify_setup(stage), check_archive):
                        with self.assertRaises(ValueError) as error:
                            check()
                        self.assertIn(name, str(error.exception))
                    file.write_bytes(b'')
                    for check in (lambda: verify_setup(stage), check_archive):
                        with self.assertRaises(ValueError):
                            check()
                    file.write_bytes(original)


if __name__ == '__main__':
    unittest.main()
