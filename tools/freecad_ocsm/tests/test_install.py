import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('ocsm_export_install', Path(__file__).resolve().parents[1] / 'install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallationTests(unittest.TestCase):
    def test_dry_run_does_not_create_directories(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'Mod'
            destination=installer.install(root,dry_run=True)
            self.assertEqual(destination.name,'OCSMExport')
            self.assertFalse(root.exists())

    def test_install_is_idempotent_and_never_replaces_other_addons(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'Mod'
            destination=installer.install(root)
            self.assertTrue(destination.is_symlink())
            self.assertEqual(installer.install(root),destination)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'Mod';other=root/'OCSMExport'
            other.mkdir(parents=True)
            (other/'user-content').write_text('preserve')
            with self.assertRaises(FileExistsError):
                installer.install(root)
            self.assertEqual((other/'user-content').read_text(),'preserve')


if __name__=='__main__':
    unittest.main()
