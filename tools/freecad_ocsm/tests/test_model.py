import math
from types import SimpleNamespace
import unittest

from ocsm_export.model import (ExportError, ExportOptions, SectionSpec,
                               STANDARD_VIEWS, ViewSpec, cross, dot)
from ocsm_export.api import options_from_techdraw
from ocsm_export.sources import view_sources


class ModelTests(unittest.TestCase):
    def test_standard_bases_are_right_handed_orthonormal(self):
        for name, view in STANDARD_VIEWS.items():
            with self.subTest(name=name):
                right, up, normal = view.basis()
                for axis in (right, up, normal):
                    self.assertAlmostEqual(dot(axis, axis), 1.0)
                self.assertAlmostEqual(dot(right, up), 0)
                self.assertAlmostEqual(dot(right, normal), 0)
                for a, b in zip(cross(right, up), normal):
                    self.assertAlmostEqual(a, b)

    def test_front_and_top_axes(self):
        self.assertEqual(STANDARD_VIEWS['front'].basis()[:2], ((1, 0, 0), (0, 0, 1)))
        self.assertEqual(STANDARD_VIEWS['top'].basis()[:2], ((1, 0, 0), (0, 1, 0)))

    def test_rotation_is_ccw_in_drawing_plane(self):
        view = ViewSpec(rotation_deg=90)
        right, up, normal = view.basis()
        self.assertAlmostEqual(dot((1, 0, 0), right), 0)
        self.assertAlmostEqual(dot((1, 0, 0), up), 1)
        self.assertAlmostEqual(dot((0, 0, 1), right), -1)

    def test_invalid_options_are_rejected(self):
        invalid = [lambda: ViewSpec(direction=(0, 0, 0)),
                   lambda: ViewSpec(up=(0, -1, 0)),
                   lambda: ViewSpec(direction=(1, 2, float('nan'))),
                   lambda: ViewSpec(rotation_deg=float('inf')),
                   lambda: ViewSpec(name=' '),
                   lambda: SectionSpec(normal=(0, 0, 0)),
                   lambda: SectionSpec(hatch_spacing=0),
                   lambda: SectionSpec(origin=(1, 2)),
                   lambda: SectionSpec(keep='both'),
                   lambda: SectionSpec(hatch_enabled=1),
                   lambda: ExportOptions(views=()),
                   lambda: ExportOptions(views=(ViewSpec(), ViewSpec())),
                   lambda: ExportOptions(mode='perspective'),
                   lambda: ExportOptions(mode='section'),
                   lambda: ExportOptions(layout='row'),
                   lambda: ExportOptions(views=(ViewSpec(mode='section'),)),
                   lambda: ViewSpec(mode='bogus'),
                   lambda: ExportOptions(mode='section', section='invalid'),
                   lambda: ExportOptions(tolerance=-1)]
        for factory in invalid:
            with self.subTest(factory=factory):
                with self.assertRaises(ValueError):
                    factory()

    def test_xsource_subelements_are_rejected_not_silently_widened(self):
        box = SimpleNamespace(Name='Box')
        view = SimpleNamespace(TypeId='TechDraw::DrawViewPart', Source=[box],
                               XSource=[(box, ['Face1', 'Edge2'])])
        with self.assertRaises(ExportError) as caught:
            view_sources(view)
        self.assertIn('sub-elements', str(caught.exception))
        self.assertIn('Face1', str(caught.exception))
        view.XSource = [(box, [])]
        view.Source = []
        self.assertEqual(view_sources(view), [box])

    def test_projgroup_rotation_vector_resolves_zero_x_direction(self):
        box = SimpleNamespace(Name='Box')
        view = SimpleNamespace(TypeId='TechDraw::DrawProjGroupItem', Name='P', Source=[box], XSource=[],
                               Direction=(0, 0, 1), XDirection=(0, 0, 0), RotationVector=(1, 0, 0),
                               Rotation=0, Perspective=False, HardHidden=False, SmoothHidden=False,
                               SeamHidden=False)
        self.assertEqual(options_from_techdraw(view).views[0].up, (0, 1, 0))
        view.XDirection = (0, 0, 1)
        with self.assertRaises(ExportError):
            options_from_techdraw(view)

    def test_section_inherits_hatch_scale_rotation_and_nocut(self):
        box = SimpleNamespace(Name='Box')
        view = SimpleNamespace(TypeId='TechDraw::DrawViewSection', Name='S', Source=[box], XSource=[],
                               Direction=(0, 0, 1), XDirection=(1, 0, 0), Rotation=0, Perspective=False,
                               HardHidden=False, SmoothHidden=False, SeamHidden=False,
                               SectionOrigin=(0, 0, 10), SectionNormal=(0, 0, 1),
                               UsePreviousCut=False, FuseBeforeCut=False, TrimAfterCut=False,
                               CutSurfaceDisplay='NoCut', HatchScale=2.0, HatchRotation=30.0)
        section = options_from_techdraw(view).section
        self.assertFalse(section.hatch_enabled)
        self.assertAlmostEqual(section.hatch_spacing, 6.35)
        # 预制图案自带 45°；75° 吸附为 90°（只允许 0/90，避免双重旋转）
        self.assertAlmostEqual(section.hatch_angle_deg, 90.0)
        view.CutSurfaceDisplay = 'Solid'
        with self.assertRaises(ExportError):
            options_from_techdraw(view)
        view.CutSurfaceDisplay = 'PatHatch'
        view.TrimAfterCut = True
        with self.assertRaises(ExportError):
            options_from_techdraw(view)

    def test_sources_do_not_import_freecad_at_module_load(self):
        # Documentation/CLI/preview imports must work outside FreeCAD.
        import ocsm_export.api
        import ocsm_export.curves
        import ocsm_export.sources
        import ocsm_export.section
        self.assertTrue(callable(ocsm_export.api.export_scene))


if __name__ == '__main__':
    unittest.main()
