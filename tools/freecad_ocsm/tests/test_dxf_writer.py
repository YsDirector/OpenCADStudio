"""Offline tests; no FreeCAD, GUI, OCS, pytest, or network is needed.

PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
    -s tools/freecad_ocsm/tests -p test_dxf_writer.py -v
"""
from __future__ import annotations

from copy import deepcopy
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

# Also supports direct unittest discovery without installing a package or
# creating __init__.py. Import only the dependency-free IR and the writer.
PACKAGE_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PACKAGE_ROOT))

import ezdxf
from ocsm_export import dxf_writer as writer
from ocsm_export.model import ExportError, LAYERS


def line(start=(0, 0), end=(10, 0), role="visible"):
    return {"type": "LINE", "role": role, "start": list(start), "end": list(end)}


def circle(radius=10, *, ccw=True, role="visible"):
    return {"type": "CIRCLE", "role": role, "center": [0, 0], "radius": radius, "ccw": ccw}


def ellipse(start=0, end=math.tau, *, ccw=True, major=(4, 3), ratio=0.4, role="thin"):
    return {"type": "ELLIPSE", "role": role, "center": [5, 7], "major_axis": list(major),
            "ratio": ratio, "start_param": start, "end_param": end, "ccw": ccw}


def spline(**kwargs):
    return {"type": "SPLINE", "role": "hidden", "degree": 2,
            "control_points": [[0, 0], [2, 3], [4, 0]], "knots": [0, 0, 0, 1, 1, 1],
            **kwargs}


def loop(edges, outer=True):
    return {"is_outer": outer, "edges": edges}


def hatch(loops=None, angle=45, spacing=3.175):
    return {"angle_deg": angle, "spacing": spacing,
            "loops": loops if loops is not None else [loop([circle(role="hatch")]),
                                                       loop([circle(4, ccw=False, role="hatch")], False)]}


def scene(entities=None, hatches=None):
    return {"schema_version": 1, "views": [{"name": "正视图", "entities": entities if entities is not None else [line()],
                                           "hatches": hatches if hatches is not None else [],
                                           "metadata": {"label": "零件甲"}}],
            "metadata": {"sources": [{"name": "轴套", "document": "中文.FCStd"}],
                         "settings": {"mode": "visible", "tolerance": 0.01}},
            "warnings": ["原始提示：请校核尺寸"]}


def ellipse_point(curve, t):
    x, y = curve["center"]
    ax, ay = curve["major_axis"]
    r = curve["ratio"]
    return x + ax * math.cos(t) - ay * r * math.sin(t), y + ay * math.cos(t) + ax * r * math.sin(t)


class DxfWriterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "中文零件.dxf"
        self.manifest = self.output.with_suffix(".manifest.json")

    def export(self, value=None, *, overwrite=False):
        value = scene() if value is None else value
        report = writer.write_dxf(value, self.output, overwrite=overwrite)
        return report, ezdxf.readfile(self.output)

    def assert_xy(self, actual, expected, places=8):
        self.assertAlmostEqual(actual[0], expected[0], places=places)
        self.assertAlmostEqual(actual[1], expected[1], places=places)

    def assert_bylayer(self, entity):
        self.assertEqual(entity.dxf.color, 256)
        self.assertEqual(entity.dxf.linetype.upper(), "BYLAYER")
        self.assertEqual(entity.dxf.lineweight, -1)
        self.assertEqual(entity.dxf.ltscale, 1)
        self.assertFalse(entity.dxf.hasattr("true_color"))

    def assert_clean(self, expected=()):
        self.assertEqual({p.name for p in self.root.iterdir()}, set(expected))

    def test_five_layers_unicode_units_complete_linetypes_and_bylayer(self):
        entities = [line((0, i), (10, i), r) for i, r in enumerate(("visible", "thin", "center", "hidden"))]
        report, doc = self.export(scene(entities, [hatch()]))
        self.assertEqual(doc.dxfversion, "AC1027")
        self.assertEqual(doc.output_encoding, "utf-8")
        for key, expected in {"$INSUNITS": 4, "$MEASUREMENT": 1, "$DIMSCALE": 1,
                              "$LTSCALE": 1, "$CELTSCALE": 1, "$TILEMODE": 1}.items():
            self.assertEqual(doc.header[key], expected)
        for role, (name, color, ltype, weight) in LAYERS.items():
            layer = doc.layers.get(name)
            self.assertEqual((layer.dxf.color, layer.dxf.linetype, layer.dxf.lineweight), (color, ltype, weight))
            self.assertEqual(report["counts"]["by_layer"][name], 1)
            self.assertEqual(report["counts"]["by_role"][role], 1)
            self.assertIn(name.encode("utf-8"), self.output.read_bytes())
        for name, expected in {"CENTER2": [15.875, -3.175, 3.175, -3.175], "DASHED2": [6.35, -1.27]}.items():
            tags = doc.linetypes.get(name).pattern_tags.tags
            self.assertEqual([t.value for t in tags if t.code == 49], expected)
            self.assertEqual([t.value for t in tags if t.code == 73], [len(expected)])
            self.assertAlmostEqual([t.value for t in tags if t.code == 40][0], sum(map(abs, expected)))
        for entity in doc.modelspace():
            self.assert_bylayer(entity)
        self.assertEqual(report["counts"]["entities"], 5)
        self.assertEqual(report["counts"]["blocks"], 0)
        self.assertFalse(doc.audit().has_errors)
        self.assertFalse(any(name.startswith("FreeCAD") for name in sys.modules))

    def test_manifest_source_settings_hash_warnings_and_input_immutability(self):
        value = scene()
        original = deepcopy(value)
        report, _ = self.export(value)
        self.assertEqual(value, original)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        for key in ("path", "manifest_path", "counts", "warnings", "placements", "sha256"):
            self.assertEqual(report[key], manifest[key])
        self.assertEqual(manifest["source"], value["metadata"]["sources"])
        self.assertEqual(manifest["settings"], value["metadata"]["settings"])
        self.assertEqual(manifest["metadata"], value["metadata"])
        self.assertEqual(manifest["views"][0]["metadata"], value["views"][0]["metadata"])
        self.assertEqual(manifest["status"], "complete")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["writer"]["ezdxf_version"], ezdxf.__version__)
        self.assertTrue(manifest["writer"]["version"])
        self.assertEqual(manifest["dxf"]["size_bytes"], self.output.stat().st_size)
        self.assertEqual(manifest["sha256"], hashlib.sha256(self.output.read_bytes()).hexdigest())
        self.assertTrue(manifest["audit"]["readback_counts_verified"])
        self.assertIn("中文", self.manifest.read_text(encoding="utf-8"))
        self.assert_clean([self.output.name, self.manifest.name])

    def test_native_curves_not_polylines_and_preview_points_ignored(self):
        curves = [line(), circle(), {"type": "ARC", "role": "center", "center": [2, 3], "radius": 5,
                                    "start_angle": 10, "end_angle": 70}, ellipse(0.4, 2.4), spline()]
        # Deliberately unrelated preview geometry must not replace true curves.
        for curve in curves:
            curve["preview_points"] = [[99, 99], [100, 100]]
        report, doc = self.export(scene(curves))
        self.assertEqual(report["counts"]["by_type"], {k: 1 for k in ("LINE", "CIRCLE", "ARC", "ELLIPSE", "SPLINE")})
        self.assertEqual({e.dxftype() for e in doc.modelspace()}, {"LINE", "CIRCLE", "ARC", "ELLIPSE", "SPLINE"})
        self.assert_xy(doc.modelspace().query("LINE")[0].dxf.start, (0, 0))
        self.assertEqual(doc.modelspace().query("CIRCLE")[0].dxf.radius, 10)

    def test_arc_world_endpoints_and_traversal_both_directions_and_wrap(self):
        cases = [(350, 10, True), (10, 350, False), (30, 120, False), (270, 90, True), (0, 360, False)]
        curves = [{"type": "ARC", "role": "visible", "center": [-3, 5], "radius": 2,
                   "start_angle": a, "end_angle": b, "ccw": ccw} for a, b, ccw in cases]
        _, doc = self.export(scene(curves))
        for entity, (a, b, ccw) in zip(doc.modelspace(), cases):
            with self.subTest(start=a, end=b, ccw=ccw):
                def point(t):
                    return -3 + 2 * math.cos(math.radians(t)), 5 + 2 * math.sin(math.radians(t))
                self.assert_xy(entity.start_point, point(a))
                self.assert_xy(entity.end_point, point(b))
                span = abs(b - a) if abs(b - a) == 360 else ((b - a) if ccw else (a - b)) % 360
                actual = list(entity.vertices([entity.dxf.start_angle + span / 2]))[0]
                self.assert_xy(actual, point(a + (1 if ccw else -1) * span / 2))
                self.assertEqual(entity.dxf.extrusion.z, 1 if ccw else -1)

    def test_ellipse_world_endpoints_and_traversal_both_directions(self):
        curves = [ellipse(0.4, 2.2), ellipse(2.2, 0.4, ccw=False),
                  ellipse(5.8, 0.2), ellipse(0, math.tau, ccw=False)]
        _, doc = self.export(scene(curves))
        for entity, curve in zip(doc.modelspace(), curves):
            with self.subTest(curve=curve):
                tool = entity.construction_tool()
                a, b, ccw = curve["start_param"], curve["end_param"], curve["ccw"]
                self.assert_xy(tool.start_point, ellipse_point(curve, a))
                self.assert_xy(tool.end_point, ellipse_point(curve, b))
                span = math.tau if abs(b - a) == math.tau else ((b - a) if ccw else (a - b)) % math.tau
                actual = list(tool.vertices([entity.dxf.start_param + span / 2]))[0]
                self.assert_xy(actual, ellipse_point(curve, a + (1 if ccw else -1) * span / 2))
                self.assertEqual(entity.dxf.extrusion.z, 1 if ccw else -1)

    def test_spline_expanded_knots_rational_closed_and_periodic(self):
        curves = [spline(control_points=[[1, 0], [1, 1], [0, 1]], knots=[-2, -2, -2, 3, 3, 3],
                         weights=[1, math.sqrt(0.5), 1]),
                  spline(degree=1, control_points=[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]],
                         knots=[0, 0, 1, 2, 3, 4, 4], closed=True),
                  spline(control_points=[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0], [2, 0]],
                         knots=list(range(9)), periodic=True)]
        _, doc = self.export(scene(curves))
        splines = list(doc.modelspace())
        for entity, curve in zip(splines, curves):
            self.assertEqual(entity.dxf.degree, curve["degree"])
            self.assertEqual(list(entity.knots), curve["knots"])
            self.assertEqual([list(p)[:2] for p in entity.control_points], curve["control_points"])
            self.assertEqual(list(entity.weights), curve.get("weights", []))
        self.assertEqual(splines[0].dxf.flags & 4, 4)
        self.assertEqual(splines[1].dxf.flags & 3, 1)
        self.assertEqual(splines[2].dxf.flags & 3, 3)
        self.assert_xy(splines[0].construction_tool().point(0.5), (math.sqrt(0.5), math.sqrt(0.5)))

    def test_hatch_circle_full_arcs_hole_flags_and_direction(self):
        report, doc = self.export(scene([], [hatch()]))
        entity = doc.modelspace().query("HATCH")[0]
        self.assertEqual(entity.dxf.hatch_style, 0)
        self.assertEqual(entity.dxf.solid_fill, 0)
        self.assertEqual(entity.dxf.associative, 0)
        self.assertEqual(len(entity.paths), 2)
        for path, outer, radius in zip(entity.paths, [True, False], [10, 4]):
            self.assertEqual(bool(path.path_type_flags & 1), outer)
            edge = path.edges[0]
            self.assertEqual(type(edge).__name__, "ArcEdge")
            self.assertEqual(edge.ccw, outer)
            self.assertEqual((edge.start_angle, edge.end_angle), (0, 360))
            self.assertEqual(edge.radius, radius)
        self.assertEqual(report["counts"]["hatch_loops"], 2)
        self.assertEqual(report["counts"]["hatch_edges_by_type"], {"ArcEdge": 2})

    def test_hatch_uses_predefined_pattern_and_only_zero_ninety_degrees(self):
        # 旧契约是"自定义图案定义 + 任意角度已烘进 offset"；新契约是
        # "预制 ANSI31（自带 45° 斜线）+ 角度只做 0/90 的零件区分"，
        # 避免定义角与 pattern_angle 同时生效造成双重旋转。
        angles = [0, 30, 45, 90, 135, 175, -30, 405]
        _, doc = self.export(scene([], [hatch(angle=a) for a in angles]))
        for entity, angle in zip(doc.modelspace(), angles):
            with self.subTest(angle=angle):
                expected = 0.0 if (angle % 180.0) < 45.0 else 90.0
                self.assertEqual(entity.dxf.pattern_name, "ANSI31")
                self.assertEqual(entity.dxf.pattern_angle, expected)
                self.assertEqual(entity.dxf.color, 256)          # ByLayer，跟图层颜色
                self.assertEqual(entity.dxf.layer, "5剖面线层")
                self.assertEqual(len(entity.pattern.lines), 1)
                pattern_line = entity.pattern.lines[0]
                # 图案自身 45° + 整体 0/90
                self.assertAlmostEqual((pattern_line.angle - 45.0 - expected) % 360.0, 0.0, places=8)

    def test_hatch_explicit_scale_is_written_without_rounding(self):
        _, doc = self.export(scene([], [{"angle_deg": 0, "spacing": 1.1e-8, "scale": 1.1e-8,
                                        "loops": hatch()["loops"]}]))
        entity = doc.modelspace()[0]
        self.assertEqual(entity.dxf.pattern_scale, 1.1e-8)
        line = entity.pattern.lines[0]
        # ANSI31 的 ISO 定义 offset = (-2.2450640303, 2.2450640303)，模长 3.175；
        # 乘 scale 后必须原样保留，不能被“旋转一次”抹掉。
        magnitude = math.hypot(line.offset[0], line.offset[1])
        self.assertAlmostEqual(magnitude, 3.1750640303 * 1.1e-8,
                               delta=1e-3 * 3.1750640303 * 1.1e-8)

    def test_hatch_partial_arc_real_endpoint_orientation(self):
        hatches = []
        for ccw in (True, False):
            arc = {"type": "ARC", "center": [0, 0], "radius": 4, "start_angle": 0 if ccw else 180,
                   "end_angle": 180 if ccw else 0, "ccw": ccw}
            closing = line((-4, 0) if ccw else (4, 0), (4, 0) if ccw else (-4, 0))
            hatches.append(hatch([loop([arc, closing])]))
        _, doc = self.export(scene([], hatches))
        for entity, ccw in zip(doc.modelspace(), [True, False]):
            path = entity.paths[0]
            arc = path.edges[0]
            self.assertEqual(arc.ccw, ccw)
            self.assert_xy(arc.real_start_point, (4, 0) if ccw else (-4, 0))
            self.assert_xy(arc.real_end_point, (-4, 0) if ccw else (4, 0))
            self.assert_xy(arc.real_end_point, path.edges[1].real_start_point)
            self.assert_xy(path.edges[1].real_end_point, arc.real_start_point)

    def test_hatch_full_ellipse_and_hole(self):
        outer = ellipse(ratio=1 / math.sqrt(3), role="hatch")
        inner = ellipse(major=(1.6, 1.2), ratio=1 / math.sqrt(3), ccw=False, role="hatch")
        report, doc = self.export(scene([], [hatch([loop([outer]), loop([inner], False)])]))
        entity = doc.modelspace()[0]
        for path, ccw in zip(entity.paths, [True, False]):
            edge = path.edges[0]
            self.assertEqual(type(edge).__name__, "EllipseEdge")
            self.assertEqual(edge.ccw, ccw)
            self.assertAlmostEqual(edge.ratio, 1 / math.sqrt(3))
            self.assertEqual((edge.start_angle, edge.end_angle), (0, 360))
        self.assertEqual(report["counts"]["hatch_edges_by_type"], {"EllipseEdge": 2})

    def test_hatch_partial_ellipse_parameters_are_not_geometric_angles(self):
        curves = [ellipse(0.4, 2.2), ellipse(2.2, 0.4, ccw=False),
                  ellipse(5.8, 0.2), ellipse(0.2, 5.8, ccw=False)]
        hatches = []
        for curve in curves:
            end = ellipse_point(curve, curve["end_param"])
            start = ellipse_point(curve, curve["start_param"])
            hatches.append(hatch([loop([curve, line(end, start)])]))
        _, doc = self.export(scene([], hatches))
        for entity, curve in zip(doc.modelspace(), curves):
            edge = entity.paths[0].edges[0]
            self.assertEqual(edge.ccw, curve["ccw"])
            self.assert_xy(edge.real_start_point, ellipse_point(curve, curve["start_param"]))
            self.assert_xy(edge.real_end_point, ellipse_point(curve, curve["end_param"]))
            param = curve["start_param"] if curve["ccw"] else curve["end_param"]
            expected = math.degrees(math.atan2(curve["ratio"] * math.sin(param), math.cos(param))) % 360
            self.assertAlmostEqual(edge.start_angle, expected)
            self.assertNotAlmostEqual(edge.start_angle, math.degrees(param) % 360)

    def test_hatch_native_rational_and_periodic_spline_edges(self):
        arch = spline(weights=[1, 0.5, 1])
        periodic = spline(control_points=[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0], [2, 0]],
                          knots=list(range(9)), periodic=True)
        report, doc = self.export(scene([], [hatch([loop([arch, line((4, 0), (0, 0))])]),
                                            hatch([loop([periodic])])]))
        a, b = [entity.paths[0].edges[0] for entity in doc.modelspace()]
        self.assertEqual(type(a).__name__, "SplineEdge")
        self.assertEqual(a.degree, 2)
        self.assertEqual(a.weights, [1, 0.5, 1])
        self.assertEqual(a.knot_values, [0, 0, 0, 1, 1, 1])
        self.assertEqual(a.rational, 1)
        self.assertEqual(b.periodic, 1)
        self.assertEqual(b.knot_values, list(range(9)))
        self.assertEqual(report["counts"]["hatch_edges_by_type"], {"LineEdge": 1, "SplineEdge": 2})

    def test_multiple_views_unscaled_blocks_keep_member_semantics(self):
        value = scene([line((-10, -5), (5, 10)), circle(role="hidden")], [hatch()])
        other = deepcopy(value["views"][0])
        other.update(name="俯视 / top", entities=[ellipse(role="center"), spline(role="thin")], hatches=[])
        value["views"].append(other)
        report, doc = self.export(value)
        inserts = list(doc.modelspace())
        self.assertEqual([e.dxftype() for e in inserts], ["INSERT", "INSERT"])
        self.assertEqual(report["counts"]["entities"], 5)
        self.assertEqual(report["counts"]["blocks"], 2)
        for placement, insert, view in zip(report["placements"], inserts, value["views"]):
            self.assertEqual(insert.dxf.layer, "0")
            self.assert_bylayer(insert)
            self.assertEqual(insert.dxf.xscale, 1)
            self.assertEqual(insert.dxf.yscale, 1)
            self.assertEqual(insert.dxf.zscale, 1)
            self.assertEqual(insert.dxf.rotation, 0)
            self.assertEqual(list(insert.dxf.insert), placement["translation"])
            block = doc.blocks.get(placement["block"])
            self.assertEqual(block.units, 4)
            for member, original in zip(block, view["entities"]):
                self.assertEqual(member.dxf.layer, LAYERS[original["role"]][0])
                self.assert_bylayer(member)
            self.assertEqual(len(list(insert.virtual_entities())), len(block))
        self.assert_xy(doc.blocks.get(inserts[0].dxf.name)[0].dxf.start, (-10, -5))
        first, second = report["placements"]
        self.assertGreaterEqual(second["bounds_mm"][0][0] - first["bounds_mm"][1][0], writer.VIEW_GAP_MM)
        self.assertEqual(first["bounds_mm"][0], [0, 0])

    def test_first_and_third_angle_layouts_align_centres_and_gaps(self):
        views=[{'name':'front','bounds':[[0,0],[40,20]]},
               {'name':'top','bounds':[[0,0],[40,30]]},
               {'name':'left','bounds':[[0,0],[30,20]]}]
        def placed(translations):
            result=[]
            for view, translation in zip(views,translations):
                (x0,y0),(x1,y1)=view['bounds']
                result.append((x0+translation[0],y0+translation[1],x1+translation[0],y1+translation[1]))
            return result
        first=placed(writer._layout_translations(views,'first_angle'))
        front,top,left=first
        self.assertAlmostEqual((front[0]+front[2])/2,(top[0]+top[2])/2)
        self.assertLessEqual(top[3]+writer.VIEW_GAP_MM,front[1])
        self.assertAlmostEqual((front[1]+front[3])/2,(left[1]+left[3])/2)
        self.assertLessEqual(front[2]+writer.VIEW_GAP_MM,left[0])
        third=placed(writer._layout_translations(views,'third_angle'))
        front,top,left=third
        self.assertLessEqual(front[3]+writer.VIEW_GAP_MM,top[1])
        self.assertLessEqual(left[2]+writer.VIEW_GAP_MM,front[0])

    def test_invalid_scenes_rejected_without_any_files(self):
        invalid = [None, {}, {"schema_version": 2, "views": []}, {"schema_version": True, "views": []},
                   scene([], []), {**scene(), "views": []}, {**scene(), "warnings": [None]},
                   {**scene(), "metadata": {"bad": float("inf")}}]
        for mutate in (lambda s: s["views"][0].pop("entities"),
                       lambda s: s["views"][0].pop("hatches"),
                       lambda s: s["views"][0].update(name=" "),
                       lambda s: s["views"].append(deepcopy(s["views"][0]))):
            value = scene()
            mutate(value)
            invalid.append(value)
        for value in invalid:
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    writer.write_dxf(value, self.output)
                self.assert_clean()

    def test_unknown_roles_types_required_fields_nonfinite_and_degeneracy(self):
        invalid = [{**line(), "type": "POLYLINE"}, {**line(), "role": "VISIBLE"},
                   {**line(), "role": "hatch"}, {**line(), "start": [float("nan"), 0]},
                   {**line(), "end": [True, 0]}, {**line(), "end": [0, 0]},
                   {**line(), "end": [0, 0, 0]}, {**line(), "ccw": False},
                   {"type": "LINE", "role": "visible", "start": [0, 0]},
                   {**circle(), "radius": -1}, {**circle(), "radius": float("inf")},
                   {**ellipse(), "ratio": 0}, {**ellipse(), "ratio": 2},
                   {**ellipse(), "major_axis": [0, 0]}, {**ellipse(), "end_param": 0},
                   {**ellipse(), "ccw": "false"},
                   {"type": "ARC", "role": "visible", "center": [0, 0], "radius": 1,
                    "start_angle": 0, "end_angle": 0}]
        invalid.append({k: v for k, v in line().items() if k != "role"})
        for curve in invalid:
            with self.subTest(curve=curve):
                with self.assertRaises(ValueError):
                    writer.write_dxf(scene([curve]), self.output)
                self.assert_clean()

    def test_invalid_splines_rejected_not_repaired(self):
        invalid = [spline(degree=0), spline(degree=True), spline(degree=12), spline(knots=[0, 1]),
                   spline(knots=[0, 0, 1, 0, 1, 1]), spline(knots=[0] * 6),
                   spline(control_points=[[0, 0]] * 3), spline(weights=[1, 1]),
                   spline(weights=[1, 0, 1]), spline(weights=[1, -1, 1]),
                   spline(weights=[1, float("inf"), 1]), spline(closed=True), spline(periodic=True),
                   spline(closed="true"), spline(ccw=False)]
        for curve in invalid:
            with self.subTest(curve=curve):
                with self.assertRaises(ValueError):
                    writer.write_dxf(scene([curve]), self.output)
                self.assert_clean()

    def test_invalid_hatches_do_not_silently_drop_edges_or_holes(self):
        invalid = [hatch([]), hatch([loop([], True)]), hatch(spacing=0), hatch(angle=float("nan")),
                   hatch([loop([circle()], False)]), hatch([loop([line()])]),
                   hatch([loop([line(), line((10, 0), (0, 0))])]),
                   hatch([loop([{**circle(), "type": "BEZIER"}])]),
                   hatch([loop([{**circle(), "role": "unknown"}])]),
                   hatch([loop([{**circle(), "role": None}])]),
                   hatch([{"is_outer": 1, "edges": [circle()]}])]
        for value in invalid:
            with self.subTest(hatch=value):
                with self.assertRaises(ValueError):
                    writer.write_dxf(scene([], [value]), self.output)
                self.assert_clean()

    def test_conflict_on_either_target_and_explicit_overwrite(self):
        self.export()
        before = (self.output.read_bytes(), self.manifest.read_bytes())
        with self.assertRaises(FileExistsError):
            self.export()
        self.assertEqual((self.output.read_bytes(), self.manifest.read_bytes()), before)
        report, doc = self.export(scene([circle()]), overwrite=True)
        self.assertEqual(doc.modelspace()[0].dxftype(), "CIRCLE")
        self.assertNotEqual(self.output.read_bytes(), before[0])
        self.assertEqual(json.loads(self.manifest.read_text())["sha256"], report["sha256"])
        self.assert_clean([self.output.name, self.manifest.name])

    def test_orphan_manifest_or_dxf_blocks_nonoverwrite_and_can_be_replaced(self):
        for occupied in (self.output, self.manifest):
            with self.subTest(occupied=occupied):
                occupied.write_bytes(b"do not clobber")
                with self.assertRaises(FileExistsError):
                    self.export()
                self.assertEqual(occupied.read_bytes(), b"do not clobber")
                self.assert_clean([occupied.name])
                self.export(overwrite=True)
                self.output.unlink()
                self.manifest.unlink()

    def test_invalid_input_preserves_existing_pair_even_with_overwrite(self):
        self.export()
        before = (self.output.read_bytes(), self.manifest.read_bytes())
        with self.assertRaises(ValueError):
            self.export(scene([{**line(), "type": "BAD"}]), overwrite=True)
        self.assertEqual((self.output.read_bytes(), self.manifest.read_bytes()), before)

    def test_stage_write_and_readback_failures_are_atomic(self):
        for existing in (False, True):
            if existing:
                self.export()
            before = {p.name: p.read_bytes() for p in self.root.iterdir()}
            for target in ("ezdxf.document.Drawing.write", "ocsm_export.dxf_writer.ezdxf.readfile"):
                with self.subTest(existing=existing, target=target):
                    with patch(target, side_effect=OSError("injected stage failure")):
                        with self.assertRaises(OSError):
                            self.export(overwrite=existing)
                    self.assertEqual({p.name: p.read_bytes() for p in self.root.iterdir()}, before)

    def test_audit_repairs_are_fatal(self):
        result = SimpleNamespace(errors=[], fixes=[SimpleNamespace(message="removed invalid entity")])
        with patch("ezdxf.document.Drawing.audit", return_value=result):
            with self.assertRaisesRegex(ExportError, "repairs"):
                self.export()
        self.assert_clean()

    def test_readback_count_mismatch_is_fatal(self):
        read = ezdxf.readfile
        def corrupted(path):
            doc = read(path)
            doc.modelspace().delete_entity(doc.modelspace()[0])
            return doc
        with patch.object(writer.ezdxf, "readfile", side_effect=corrupted):
            with self.assertRaisesRegex(ExportError, "counts differ"):
                writer.write_dxf(scene(), self.output)
        self.assert_clean()

    def test_second_publication_failure_rolls_back_new_and_overwritten_pairs(self):
        real_link = os.link
        def fail_manifest(src, dst, *args, **kwargs):
            if Path(dst) == self.manifest:
                raise OSError("injected manifest commit failure")
            return real_link(src, dst, *args, **kwargs)
        for existing in (False, True):
            if existing:
                self.export()
            before = {p.name: p.read_bytes() for p in self.root.iterdir()}
            with patch.object(writer.os, "link", side_effect=fail_manifest):
                with self.assertRaises(OSError):
                    self.export(scene([circle()]), overwrite=existing)
            self.assertEqual({p.name: p.read_bytes() for p in self.root.iterdir()}, before)

    def test_directory_fsync_failure_after_publication_rolls_back(self):
        real_sync = writer._sync_directory
        for existing in (False, True):
            if existing:
                self.export()
            before = {p.name: p.read_bytes() for p in self.root.iterdir()}
            calls = 0
            def fail_once(path):
                nonlocal calls
                calls += 1
                if calls == (3 if existing else 1):
                    raise OSError("injected directory fsync failure")
                return real_sync(path)
            with self.subTest(existing=existing):
                with patch.object(writer, "_sync_directory", side_effect=fail_once):
                    with self.assertRaises(OSError):
                        self.export(scene([circle()]), overwrite=existing)
                self.assertEqual({p.name: p.read_bytes() for p in self.root.iterdir()}, before)

    def test_post_commit_cleanup_failure_reports_success_with_warning(self):
        with patch.object(writer.shutil, "rmtree", side_effect=OSError("cleanup unavailable")):
            report, doc = self.export()
        self.assertEqual(len(doc.modelspace()), 1)
        self.assertIn("Export committed", report["warnings"][-1])
        self.assertIn("cleanup unavailable", report["warnings"][-1])
        manifest = json.loads(self.manifest.read_text())
        self.assertEqual(manifest["sha256"], hashlib.sha256(self.output.read_bytes()).hexdigest())
        self.assertFalse(self.output.with_name(f".{self.output.name}.writer.lock").exists())

    def test_overwrite_dxf_replace_failure_restores_withdrawn_manifest(self):
        self.export()
        before = (self.output.read_bytes(), self.manifest.read_bytes())
        replace = os.replace
        def fail_dxf(src, dst):
            if Path(src).name == "payload.dxf":
                raise OSError("injected DXF replacement failure")
            return replace(src, dst)
        with patch.object(writer.os, "replace", side_effect=fail_dxf):
            with self.assertRaises(OSError):
                self.export(scene([circle()]), overwrite=True)
        self.assertEqual((self.output.read_bytes(), self.manifest.read_bytes()), before)
        self.assert_clean([self.output.name, self.manifest.name])

    def test_nooverwrite_race_never_clobbers_an_external_target(self):
        real_link = os.link
        for conflict in (self.output, self.manifest):
            def race(src, dst, *args, **kwargs):
                if Path(dst) == conflict:
                    conflict.write_bytes(b"external writer")
                return real_link(src, dst, *args, **kwargs)
            with self.subTest(conflict=conflict):
                with patch.object(writer.os, "link", side_effect=race):
                    with self.assertRaises(FileExistsError):
                        self.export()
                self.assertEqual(conflict.read_bytes(), b"external writer")
                self.assert_clean([conflict.name])
                conflict.unlink()

    def test_exclusive_lock_is_not_removed_by_another_writer(self):
        lock = self.output.with_name(f".{self.output.name}.writer.lock")
        lock.write_text("other pid")
        with self.assertRaises(FileExistsError):
            self.export(overwrite=True)
        self.assertEqual(lock.read_text(), "other pid")
        self.assert_clean([lock.name])

    def test_rollback_failure_retains_recovery_files_and_no_success_manifest(self):
        self.export()
        old_dxf, old_manifest = self.output.read_bytes(), self.manifest.read_bytes()
        real_link, real_replace = os.link, os.replace
        def fail_commit(src, dst, *args, **kwargs):
            if Path(dst) == self.manifest:
                raise OSError("commit failure")
            return real_link(src, dst, *args, **kwargs)
        def fail_restore(src, dst):
            if Path(src).name == "previous.dxf":
                raise OSError("restore failure")
            return real_replace(src, dst)
        with patch.object(writer.os, "link", side_effect=fail_commit), patch.object(writer.os, "replace", side_effect=fail_restore):
            with self.assertRaisesRegex(ExportError, "recovery files retained"):
                self.export(scene([circle()]), overwrite=True)
        self.assertFalse(self.manifest.exists())
        stages = list(self.root.glob(".*.stage-*"))
        self.assertEqual(len(stages), 1)
        self.assertEqual((stages[0] / "previous.dxf").read_bytes(), old_dxf)
        self.assertEqual((stages[0] / "previous.manifest.json").read_bytes(), old_manifest)
        self.assertTrue(self.output.with_name(f".{self.output.name}.writer.lock").exists())

    def test_bad_paths_symlinks_and_non_boolean_overwrite(self):
        with self.assertRaises(ValueError):
            writer.write_dxf(scene(), self.root / "x.manifest.json")
        with self.assertRaises(FileNotFoundError):
            writer.write_dxf(scene(), self.root / "missing" / "x.dxf")
        with self.assertRaises(ValueError):
            writer.write_dxf(scene(), self.output, overwrite="yes")
        target = self.root / "target"
        target.write_bytes(b"untouched")
        for path in (self.output, self.manifest):
            path.symlink_to(target)
            with self.assertRaises(ValueError):
                self.export(overwrite=True)
            self.assertTrue(path.is_symlink())
            path.unlink()
        self.assertEqual(target.read_bytes(), b"untouched")

    def test_cli_json_only_success_conflict_overwrite_and_errors(self):
        input_path = self.root / "输入.json"
        input_path.write_text(json.dumps(scene(), ensure_ascii=False), encoding="utf-8")
        env = {**os.environ, "PYTHONPATH": str(PACKAGE_ROOT), "PYTHONDONTWRITEBYTECODE": "1"}
        command = [sys.executable, "-m", "ocsm_export.writer_cli", str(input_path), str(self.output)]
        def run(extra=()):
            return subprocess.run(command + list(extra), env=env, capture_output=True, text=True, timeout=30)
        result = run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        report = json.loads(result.stdout)
        self.assertEqual(report["counts"]["entities"], 1)
        self.assertEqual(result.stdout.count("\n"), 1)
        result = run()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(json.loads(result.stderr)["type"], "FileExistsError")
        result = run(["--overwrite"])
        self.assertEqual(result.returncode, 0, result.stderr)
        for text in ("{broken", json.dumps({"schema_version": 999, "views": []})):
            input_path.write_text(text, encoding="utf-8")
            result = run(["--overwrite"])
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
            self.assertIn("error", json.loads(result.stderr))


if __name__ == "__main__":
    unittest.main()
