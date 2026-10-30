"""Options and entrypoint tests only: no real/fake Qt windows are instantiated."""
import ast
from dataclasses import replace
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
from types import ModuleType
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from ocsm_export.gui import (camera_view, form_from_options, options_from_form,
                             output_conflicts, target_path)
from ocsm_export.model import ExportOptions, SectionSpec, STANDARD_VIEWS, ViewSpec


class GuiOptionsTests(unittest.TestCase):
    def test_defaults_are_exact_and_approximation_is_opt_in(self):
        options = options_from_form({})
        self.assertEqual(options, ExportOptions())
        self.assertFalse(options.allow_approximation)
        self.assertTrue(options.include_cosmetics)  # safety rejection, not claimed support
        self.assertTrue(options.hidden_hard)
        self.assertIsNone(options.hidden_smooth)
        self.assertIsNone(options.hidden_seams)

    def test_all_seven_standard_directions(self):
        options = options_from_form({"views": list(STANDARD_VIEWS)})
        self.assertEqual(options.views, tuple(STANDARD_VIEWS.values()))
        self.assertEqual(len(options.views), 7)

    def test_camera_direction_is_negated_but_up_is_preserved(self):
        view = camera_view((0, 0, -2), (0, 1, 0))
        self.assertEqual(view.direction, (0, 0, 1))
        self.assertEqual(view.up, (0, 1, 0))
        self.assertEqual(view.basis()[0], (1, 0, 0))
        options = options_from_form({"views": ["camera"]}, camera_direction=(0, 1, 0), camera_up=(0, 0, 1))
        self.assertEqual(options.views[0].direction, (0, -1, 0))

    def test_camera_roll_is_preserved(self):
        view = camera_view((0, 0, -1), (1, 0, 0))
        self.assertEqual(view.basis()[1], (1, 0, 0))
        self.assertEqual(view.basis()[0], (0, -1, 0))

    def test_custom_vectors_and_rotation(self):
        options = options_from_form(dict(views=["custom"], custom_name="my view",
                                         direction="0， -3， 0", up="0 0 2", rotation_deg="27.125"))
        self.assertEqual(options.views[0], ViewSpec("my view", (0, -1, 0), (0, 0, 1), 27.125))

    def test_three_modes_and_section_parameters(self):
        for mode in ("visible", "hidden", "section"):
            with self.subTest(mode=mode):
                options = options_from_form(dict(mode=mode, include_smooth=True, include_seams=True,
                                                 section_origin="1, 2, 3", section_normal="1, 0, 0",
                                                 section_keep="positive", section_hidden=True,
                                                 hatch_angle_deg="-30.5", hatch_spacing="2.75",
                                                 tolerance="0.002", allow_approximation=True))
                self.assertEqual(options.mode, mode)
                self.assertTrue(options.include_smooth)
                self.assertTrue(options.include_seams)
                self.assertTrue(options.section_hidden)
                self.assertEqual(options.tolerance, 0.002)
                self.assertTrue(options.allow_approximation)
                if mode == "section":
                    self.assertEqual(options.section, SectionSpec((1, 2, 3), (1, 0, 0), "positive", -30.5, 2.75))
                else:
                    self.assertIsNone(options.section)

    def test_explicit_cosmetics_opt_out_is_preserved(self):
        options = options_from_form({"include_cosmetics": False})
        self.assertFalse(options.include_cosmetics)
        self.assertFalse(form_from_options(options)["include_cosmetics"])
        self.assertEqual(options_from_form(form_from_options(options)), options)

    def test_imported_techdraw_hidden_categories_remain_independent(self):
        original = ExportOptions(views=(ViewSpec("DrawingView", (0, -1, 0), (0, 0, 1), 17.123456789),),
                                 mode="hidden", include_smooth=True, include_seams=False,
                                 hidden_hard=False, hidden_smooth=False, hidden_seams=True)
        form = form_from_options(original)
        self.assertEqual(form["views"], ["custom"])
        self.assertEqual(form["custom_name"], "DrawingView")
        self.assertEqual(options_from_form(form), original)
        # Editing visible categories must not rewrite explicit inherited hidden flags.
        form["include_smooth"] = False
        form["include_seams"] = True
        changed = options_from_form(form)
        self.assertFalse(changed.hidden_hard)
        self.assertFalse(changed.hidden_smooth)
        self.assertTrue(changed.hidden_seams)

    def test_tri_state_hidden_switches_and_section_round_trip(self):
        for hidden in (None, False, True):
            with self.subTest(hidden=hidden):
                original = ExportOptions(views=(ViewSpec("Section001"),), mode="section",
                                         include_smooth=True, include_seams=True,
                                         section=SectionSpec((1.25, -2.5, 3), (0, -1, 0), "positive", 0.0, 1.23456789),
                                         section_hidden=True, tolerance=0.00000123456789,
                                         hidden_hard=True, hidden_smooth=hidden, hidden_seams=hidden)
                self.assertEqual(options_from_form(form_from_options(original)), original)

    def test_per_view_modes_and_layout_round_trip(self):
        original = ExportOptions(
            views=(replace(STANDARD_VIEWS['front'], mode='visible'),
                   replace(STANDARD_VIEWS['top'], mode='section', section_hidden=True)),
            mode='visible', layout='third_angle', section=SectionSpec((0, 0, 10), (0, 0, 1)))
        self.assertEqual(options_from_form(form_from_options(original)), original)
        with self.assertRaises(ValueError):
            options_from_form(dict(views=['front'], layout='row'))

    def test_preset_plus_one_custom_round_trip(self):
        original = ExportOptions(views=(STANDARD_VIEWS["top"], ViewSpec("TD001"), STANDARD_VIEWS["iso"]))
        self.assertEqual(options_from_form(form_from_options(original)), original)

    def test_two_arbitrary_views_are_rejected_not_silently_dropped(self):
        options = ExportOptions(views=(ViewSpec("A"), ViewSpec("B")))
        with self.assertRaisesRegex(ValueError, "一个自定义"):
            form_from_options(options)

    def test_invalid_views(self):
        cases = [dict(views=[]), dict(views="front"), dict(views=[None]), dict(views=["unknown"]),
                 dict(views=["front", "front"]), dict(views=["camera"]),
                 dict(views=["custom"], custom_name=""),
                 dict(views=["front", "custom"], custom_name="front"),
                 dict(views=["custom"], direction="0,0,0"),
                 dict(views=["custom"], direction="0,0,1", up="0,0,3"),
                 dict(views=["custom"], up="0,1"), dict(views=["custom"], rotation_deg="nan")]
        for values in cases:
            with self.subTest(values=values), self.assertRaises(ValueError):
                options_from_form(values)

    def test_bad_flags_numbers_and_sections(self):
        cases = [None, [], {"mode": "bad"}, {"mode": []}, {"tolerance": 0}, {"tolerance": float("inf")},
                 {"allow_approximation": "false"}, {"include_cosmetics": 0}, {"hidden_hard": None},
                 {"hidden_smooth": "false"}, {"hidden_seams": 1},
                 {"mode": "section", "section_keep": "both"},
                 {"mode": "section", "section_origin": "0, nan, 0"},
                 {"mode": "section", "section_normal": "0, 0, 0"},
                 {"mode": "section", "hatch_spacing": -1}, {"mode": "section", "hatch_angle_deg": True}]
        for values in cases:
            with self.subTest(values=values), self.assertRaises(ValueError):
                options_from_form(values)

    def test_target_validation_makes_no_files(self):
        with tempfile.TemporaryDirectory(prefix="ocsm-gui-test-") as folder:
            base = Path(folder)
            path = target_path(str(base / "demo"))
            self.assertEqual(path, base / "demo.dxf")
            self.assertEqual(target_path(str(base / "demo.DXF")).suffix, ".DXF")
            self.assertFalse(path.exists())
            self.assertEqual(list(base.iterdir()), [])
            with self.assertRaisesRegex(ValueError, "目录"):
                target_path(str(base))
        for value in (None, "", "  ", "output.svg"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                target_path(value)

    def test_both_dxf_and_sidecar_require_confirmation_including_sidecar_only(self):
        with tempfile.TemporaryDirectory(prefix="ocsm-gui-test-") as folder:
            path = Path(folder) / "drawing.dxf"
            manifest = path.with_suffix(".manifest.json")
            self.assertEqual(output_conflicts(path), ())
            manifest.write_text("existing audit", encoding="utf-8")
            self.assertEqual(output_conflicts(path), (manifest,))
            path.write_text("existing drawing", encoding="utf-8")
            self.assertEqual(output_conflicts(path), (path, manifest))
            self.assertEqual(path.read_text(encoding="utf-8"), "existing drawing")
            self.assertEqual(manifest.read_text(encoding="utf-8"), "existing audit")

    def test_preflight_rejects_directories_and_dangling_symlinks(self):
        with tempfile.TemporaryDirectory(prefix="ocsm-gui-test-") as folder:
            path = Path(folder) / "drawing.dxf"
            path.mkdir()
            with self.assertRaises(ValueError):
                output_conflicts(path)
            path.rmdir()
            try:
                path.symlink_to(Path(folder) / "missing")
            except (OSError, NotImplementedError):
                self.skipTest("symlinks unavailable")
            with self.assertRaisesRegex(ValueError, "符号链接"):
                output_conflicts(path)


class EntrypointTests(unittest.TestCase):
    def test_import_does_not_request_freecad_qt_or_ezdxf(self):
        script = '''
import builtins, sys
original = builtins.__import__
def guarded(name, *args, **kwargs):
    if name.split('.')[0] in {'FreeCAD', 'FreeCADGui', 'PySide', 'PySide2', 'PySide6', 'ezdxf', 'numpy'}:
        raise AssertionError('Forbidden import: ' + name)
    return original(name, *args, **kwargs)
builtins.__import__ = guarded
from ocsm_export.gui import options_from_form, show
from ocsm_export.preview import scene_svg
assert not options_from_form({}).allow_approximation
assert '<svg' in scene_svg({'schema_version': 1, 'views': []})
assert 'ocsm_export.api' not in sys.modules
assert 'ocsm_export.dxf_writer' not in sys.modules
'''
        env = {**os.environ, "PYTHONPATH": str(ROOT)}
        result = subprocess.run([sys.executable, "-c", script], env=env, capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_init_only_adds_current_plugin_path_even_without_dunder_file(self):
        before = list(sys.path)
        try:
            sys.path[:] = [path for path in sys.path if path != str(ROOT)]
            source = (ROOT / "Init.py").read_text(encoding="utf-8")
            namespace = {}  # mirrors FreeCAD's exec(compile(...)) startup loader
            exec(compile(source, str(ROOT / "Init.py"), "exec"), namespace)
            self.assertEqual(sys.path[0], str(ROOT))
            exec(compile(source, str(ROOT / "Init.py"), "exec"), {})
            self.assertEqual(sys.path.count(str(ROOT)), 1)
        finally:
            sys.path[:] = before

    def test_workbench_registration_uses_official_python_pattern_without_writer_import(self):
        calls = []
        gui = ModuleType("FreeCADGui")
        gui.addCommand = lambda name, command: calls.append(("command", name, command))
        gui.addWorkbench = lambda wb: calls.append(("workbench", wb))
        app = ModuleType("FreeCAD")
        app.ActiveDocument = None

        class Workbench:
            def appendToolbar(self, name, commands):
                calls.append(("toolbar", name, commands))

            def appendMenu(self, name, commands):
                calls.append(("menu", name, commands))

        with patch.dict(sys.modules, {"FreeCADGui": gui, "FreeCAD": app}):
            runpy.run_path(str(ROOT / "InitGui.py"), init_globals={"Workbench": Workbench})
            command_call = next(call for call in calls if call[0] == "command")
            self.assertEqual(command_call[1], "OCSMExport")
            self.assertFalse(command_call[2].IsActive())
            app.ActiveDocument = object()
            self.assertTrue(command_call[2].IsActive())
            wb = next(call[1] for call in calls if call[0] == "workbench")
            self.assertEqual(type(wb).__name__, "OCSMExport")
            self.assertEqual(wb.GetClassName(), "Gui::PythonWorkbench")
            wb.Initialize()
        self.assertIn(("menu", "OCSMExport", ["OCSMExport"]), calls)
        self.assertIn(("toolbar", "OCSMExport", ["OCSMExport"]), calls)

    def test_macro_is_syntax_checked_not_run_and_has_relocatable_install_guidance(self):
        source = (ROOT / "OCSMExport.FCMacro").read_text(encoding="utf-8")
        ast.parse(source, filename="OCSMExport.FCMacro")
        self.assertNotIn("/home/", source)
        self.assertIn("OCSM_EXPORT_ROOT", source)
        self.assertIn("__ModDirs__", source)
        self.assertIn("不会联网下载或自动安装", source)


if __name__ == "__main__":
    unittest.main()
