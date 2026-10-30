from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import unittest

from ocsm_export.api import export_scene
from ocsm_export.model import ExportError


def simple_scene():
    return {'schema_version':1,'views':[{'name':'front','entities':[
        {'type':'LINE','role':'visible','start':[0,0],'end':[40,0]}],
        'hatches':[],'metadata':{}}],'metadata':{'units':'mm'},'warnings':[]}


class PublicAPITests(unittest.TestCase):
    def test_direct_and_external_writer_bridge_agree(self):
        with tempfile.TemporaryDirectory() as temp:
            direct=export_scene(simple_scene(),Path(temp)/'direct.dxf')
            external=export_scene(simple_scene(),Path(temp)/'external.dxf',python_executable=sys.executable)
            self.assertEqual(direct['counts'],external['counts'])
            self.assertTrue(Path(external['manifest_path']).is_file())

    def test_external_failure_does_not_publish_output(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'failed.dxf'
            with self.assertRaises(ExportError):
                export_scene(simple_scene(),path,python_executable=str(Path(temp)/'not-a-python'))
            self.assertFalse(path.exists())

    def test_output_extension_must_match_format(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                export_scene(simple_scene(),Path(temp)/'mislabelled.dwg')
            self.assertEqual(list(Path(temp).iterdir()),[])

    def test_external_writer_preserves_no_overwrite_policy(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'same.dxf'
            first=export_scene(simple_scene(),path)
            with self.assertRaises(ExportError):
                export_scene(simple_scene(),path,python_executable=sys.executable)
            import hashlib
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(),first['sha256'])
    def test_overwrite_must_be_a_real_bool_before_any_write(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'strict.dxf'
            for value in (1,0,'false','yes',None):
                with self.subTest(value=value), self.assertRaises(ValueError):
                    export_scene(simple_scene(),path,overwrite=value,python_executable=sys.executable)
            self.assertEqual(list(Path(temp).iterdir()),[])

    def test_symlink_targets_are_refused_without_touching_the_real_file(self):
        with tempfile.TemporaryDirectory() as temp:
            real=Path(temp)/'real.dxf'
            real.write_text('do-not-touch',encoding='utf-8')
            link=Path(temp)/'link.dxf'
            try:
                link.symlink_to(real)
            except OSError as exc:
                self.skipTest(f'cannot create symlink here: {exc}')
            for kwargs in ({},{'python_executable':sys.executable}):
                with self.subTest(writer=kwargs):
                    with self.assertRaises(ExportError):
                        export_scene(simple_scene(),link,overwrite=True,**kwargs)
                    self.assertTrue(link.is_symlink())
                    self.assertEqual(real.read_text(encoding='utf-8'),'do-not-touch')
            dangling=Path(temp)/'dangling.dxf'
            dangling.symlink_to(Path(temp)/'missing.dxf')
            with self.assertRaises(ExportError):
                export_scene(simple_scene(),dangling,python_executable=sys.executable)
            self.assertTrue(dangling.is_symlink())
            self.assertFalse((Path(temp)/'missing.dxf').exists())
    def test_writer_cli_emits_utf8_json_with_ascii_stdio(self):
        root=Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temp:
            scene_path=Path(temp)/'scene.json'
            out=Path(temp)/'utf8.dxf'
            scene_path.write_text(json.dumps(simple_scene(),ensure_ascii=False),encoding='utf-8')
            env={**os.environ,'PYTHONPATH':str(root),'PYTHONIOENCODING':'ascii'}
            result=subprocess.run([sys.executable,'-m','ocsm_export.writer_cli',str(scene_path),str(out)],
                                  env=env,capture_output=True,text=True,encoding='utf-8',timeout=60)
            self.assertEqual(result.returncode,0,result.stderr)
            report=json.loads(result.stdout)
            self.assertIn('1轮廓实线层',report['counts']['by_layer'])
            self.assertTrue(out.is_file())


if __name__=='__main__':
    unittest.main()
