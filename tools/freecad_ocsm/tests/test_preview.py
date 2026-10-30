"""Pure stdlib tests. No FreeCAD, Qt application, display, or DXF dependency."""
from copy import deepcopy
import math
from pathlib import Path
import re
import sys
import unittest
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ocsm_export.preview import scene_summary, scene_svg

NS = {"s": "http://www.w3.org/2000/svg"}


def scene(entities=(), hatches=(), name="front", **extra):
    return {"schema_version": 1, "views": [{"name": name, "entities": list(entities),
                                           "hatches": list(hatches), "metadata": {}}],
            "warnings": [], "metadata": {}, **extra}


def line(a=(0, 0), b=(10, 5), role="visible", **extra):
    return dict(type="LINE", start=list(a), end=list(b), role=role, **extra)


def loop(points, outer, role="hatch"):
    return {"is_outer": outer, "edges": [line(a, b, role) for a, b in zip(points, points[1:] + points[:1])]}


def holed_hatch():
    return {"loops": [loop([(0, 0), (20, 0), (20, 20), (0, 20)], True),
                      loop([(5, 5), (5, 15), (15, 15), (15, 5)], False)],
            "angle_deg": 30, "spacing": 2.5}


def paths(root, role):
    return root.findall(f'.//s:path[@data-role="{role}"]', NS)


def path_points(path):
    numbers = [float(v) for v in re.findall(r"[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?", path.attrib["d"])]
    return list(zip(numbers[::2], numbers[1::2]))


class PreviewTests(unittest.TestCase):
    def test_svg_structure_units_dimensions_and_no_mutation(self):
        value = scene([line()])
        before = deepcopy(value)
        root = ET.fromstring(scene_svg(value))
        self.assertEqual(root.tag, "{" + NS["s"] + "}svg")
        self.assertEqual(root.attrib["width"], "900")
        self.assertEqual(root.attrib["height"], "650")
        self.assertEqual(root.attrib["viewBox"], "0 0 900 650")
        self.assertIn("10 × 5 mm", "".join(root.itertext()))
        self.assertIn("1轮廓实线层", scene_summary(value))
        self.assertEqual(paths(root, "visible")[0].attrib["d"], "M 0 0 L 10 5")
        self.assertEqual(value, before)

    def test_preview_points_have_priority(self):
        value = scene([dict(type="SPLINE", role="thin", degree=3,
                            control_points=[[10000, 10000], [20000, 20000]],
                            preview_points=[[0, 0], [2, 3], [4, 0]])])
        root = ET.fromstring(scene_svg(value))
        self.assertEqual(path_points(paths(root, "thin")[0]), [(0, 0), (2, 3), (4, 0)])
        self.assertNotIn("10000", paths(root, "thin")[0].attrib["d"])
        self.assertIn("4 × 3 mm", scene_summary(value))

    def test_no_spline_control_polygon_without_samples(self):
        value = scene([dict(type="SPLINE", role="visible", control_points=[[0, 0], [2, 7], [4, 0]])])
        root = ET.fromstring(scene_svg(value))
        self.assertFalse(paths(root, "visible"))
        self.assertIsNotNone(root.find('.//s:text[@data-placeholder="true"]', NS))
        self.assertIn("SPLINE", root.findtext("s:desc", namespaces=NS))
        self.assertIn("preview_points", scene_summary(value))
        self.assertIn("未绘制控制多边形", scene_summary(value))

    def test_unknown_unsampled_type_is_explicit(self):
        value = scene([{"type": "FUTURE_CURVE"}])
        self.assertIn("FUTURE_CURVE", scene_summary(value))
        self.assertIn('data-placeholder="true"', scene_svg(value))

    def test_hidden_and_center_dash_patterns(self):
        root = ET.fromstring(scene_svg(scene([line(role=role) for role in ("visible", "hidden", "thin", "center")])))
        self.assertNotIn("stroke-dasharray", paths(root, "visible")[0].attrib)
        self.assertNotIn("stroke-dasharray", paths(root, "thin")[0].attrib)
        self.assertEqual(len(paths(root, "hidden")[0].attrib["stroke-dasharray"].split()), 2)
        self.assertEqual(len(paths(root, "center")[0].attrib["stroke-dasharray"].split()), 4)
        for role, index in (("visible", 1), ("thin", 2), ("center", 3), ("hidden", 4)):
            self.assertTrue(paths(root, role)[0].attrib["data-layer"].startswith(str(index)))

    def test_circle_fallback_is_closed_and_has_mm_bounds(self):
        value = scene([dict(type="CIRCLE", center=[5, -2], radius=2)])
        root = ET.fromstring(scene_svg(value))
        curve = paths(root, "visible")[0]
        self.assertTrue(curve.attrib["d"].endswith(" Z"))
        self.assertGreater(len(path_points(curve)), 60)
        self.assertIn("4 × 4 mm", scene_summary(value))

    def test_arc_fallback_respects_clockwise(self):
        arc = dict(type="ARC", center=[0, 0], radius=10, start_angle=90, end_angle=0, ccw=False)
        points = path_points(paths(ET.fromstring(scene_svg(scene([arc]))), "visible")[0])
        self.assertAlmostEqual(points[0][0], 0, places=8)
        self.assertAlmostEqual(points[0][1], 10)
        self.assertAlmostEqual(points[-1][0], 10)
        self.assertAlmostEqual(points[-1][1], 0, places=8)
        self.assertTrue(all(x >= -1e-8 and y >= -1e-8 for x, y in points))
        self.assertGreater(points[1][0], 0)

    def test_arc_wrapped_interval(self):
        arc = dict(type="ARC", center=[0, 0], radius=1, start_angle=350, end_angle=10)
        points = path_points(paths(ET.fromstring(scene_svg(scene([arc]))), "visible")[0])
        self.assertTrue(all(x > 0.98 for x, _ in points))
        self.assertLess(points[0][1], 0)
        self.assertGreater(points[-1][1], 0)

    def test_ellipse_fallback_rotated_axis_and_radian_interval(self):
        ellipse = dict(type="ELLIPSE", center=[0, 0], major_axis=[0, 4], ratio=0.5,
                       start_param=0, end_param=math.tau, ccw=False)
        value = scene([ellipse])
        root = ET.fromstring(scene_svg(value))
        self.assertIn("4 × 8 mm", scene_summary(value))
        points = path_points(paths(root, "visible")[0])
        self.assertAlmostEqual(points[0][1], 4)
        self.assertGreater(points[1][0], 0)
        self.assertTrue(paths(root, "visible")[0].attrib["d"].endswith(" Z"))

    def test_hatch_hole_uses_evenodd_and_actual_pattern(self):
        value = scene(hatches=[holed_hatch()])
        root = ET.fromstring(scene_svg(value))
        hatch = paths(root, "hatch")[0]
        self.assertEqual(hatch.attrib["fill-rule"], "evenodd")
        self.assertEqual(hatch.attrib["data-outer-count"], "1")
        self.assertEqual(hatch.attrib["data-inner-count"], "1")
        self.assertEqual(hatch.attrib["d"].count("M "), 2)
        self.assertEqual(hatch.attrib["d"].count(" Z"), 2)
        pattern = root.find(".//s:pattern", NS)
        self.assertEqual(pattern.attrib["patternUnits"], "userSpaceOnUse")
        self.assertEqual(pattern.attrib["width"], "2.5")
        self.assertEqual(pattern.attrib["height"], "2.5")
        self.assertEqual(pattern.attrib["patternTransform"], "rotate(30)")
        self.assertEqual(hatch.attrib["fill"], "url(#" + pattern.attrib["id"] + ")")
        self.assertIn("5剖面线层: 1", scene_summary(value))

    def test_hatch_circle_boundaries_without_role(self):
        hatch = dict(loops=[dict(is_outer=True, edges=[dict(type="CIRCLE", center=[0, 0], radius=10)]),
                            dict(is_outer=False, edges=[dict(type="CIRCLE", center=[0, 0], radius=4, ccw=False)])],
                     angle_deg=45, spacing=1)
        root = ET.fromstring(scene_svg(scene(hatches=[hatch])))
        self.assertEqual(len(paths(root, "hatch")), 1)
        self.assertEqual(paths(root, "hatch")[0].attrib["data-inner-count"], "1")

    def test_hatch_multiple_outer_islands(self):
        hatch = holed_hatch()
        hatch["loops"].append(loop([(30, 0), (40, 0), (40, 10), (30, 10)], True))
        path = paths(ET.fromstring(scene_svg(scene(hatches=[hatch]))), "hatch")[0]
        self.assertEqual(path.attrib["data-outer-count"], "2")
        self.assertEqual(path.attrib["d"].count("M "), 3)

    def test_missing_hole_geometry_omits_whole_hatch(self):
        hatch = holed_hatch()
        hatch["loops"][1]["edges"] = [dict(type="SPLINE", control_points=[[5, 5], [10, 10]], role="hatch")]
        value = scene(hatches=[hatch])
        root = ET.fromstring(scene_svg(value))
        self.assertFalse(paths(root, "hatch"))
        self.assertIn("整个 HATCH 已省略", scene_summary(value))

    def test_discontinuous_hatch_omitted_instead_of_bridged(self):
        hatch = holed_hatch()
        hatch["loops"][0]["edges"][1]["start"] = [30, 10]
        value = scene(hatches=[hatch])
        self.assertFalse(paths(ET.fromstring(scene_svg(value)), "hatch"))
        self.assertIn("环不闭合", scene_summary(value))

    def test_names_and_warnings_are_xml_escaped(self):
        name = '<script>bad()</script> & " onload="evil"'
        warning = '<image href="file:///secret"/> & things'
        value = scene([line()], name=name, warnings=[warning])
        svg = scene_svg(value)
        root = ET.fromstring(svg)
        self.assertIn("&lt;script&gt;", svg)
        self.assertIn("&quot;", svg)
        self.assertIn("&amp;", svg)
        self.assertFalse(root.findall(".//s:script", NS))
        self.assertFalse(root.findall(".//s:image", NS))
        self.assertEqual(root.find('.//s:g[@data-view]', NS).attrib["data-view"], name)
        self.assertIn(warning, root.findtext("s:desc", namespaces=NS))

    def test_invalid_xml_control_characters_are_sanitized(self):
        root = ET.fromstring(scene_svg(scene([line()], name="a\x00b\ud800")))
        self.assertIn("a\ufffdb\ufffd", "".join(root.itertext()))

    def test_empty_scene_and_empty_view(self):
        value = {"schema_version": 1, "views": []}
        root = ET.fromstring(scene_svg(value))
        self.assertIn("No views", "".join(root.itertext()))
        self.assertIn("空场景", scene_summary(value))
        root = ET.fromstring(scene_svg(scene()))
        self.assertIn("No geometry", "".join(root.itertext()))
        self.assertFalse(root.findall(".//s:path", NS))

    def test_multiple_views_have_distinct_panels_and_definition_ids(self):
        value = scene([line()], [holed_hatch()], name="A")
        second = scene([line((100, 100), (101, 102))], [holed_hatch()], name="B")["views"][0]
        value["views"].append(second)
        root = ET.fromstring(scene_svg(value))
        panels = root.findall('.//s:g[@data-view]', NS)
        self.assertEqual([p.attrib["data-view"] for p in panels], ["A", "B"])
        ids = [e.attrib["id"] for e in root.iter() if "id" in e.attrib]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(len(root.findall(".//s:clipPath", NS)), 2)
        self.assertEqual(len(root.findall(".//s:pattern", NS)), 2)
        self.assertIn("A:", scene_summary(value))
        self.assertIn("B:", scene_summary(value))

    def test_y_is_flipped_and_geometry_is_fitted_inside_clip(self):
        value = scene([line((-100, -50), (200, 70))])
        root = ET.fromstring(scene_svg(value, width=800, height=500))
        geom = root.find('.//s:g[@data-units="mm"]', NS)
        tx, ty, sx, sy = [float(n) for n in re.findall(r"[-+]?\d+(?:\.\d+)?(?:e[-+]?\d+)?", geom.attrib["transform"])]
        self.assertGreater(sx, 0)
        self.assertEqual(sx, -sy)
        self.assertLess(ty + sy * 70, ty + sy * -50)
        rect = root.find(".//s:clipPath/s:rect", NS)
        x, y, w, h = [float(rect.attrib[k]) for k in ("x", "y", "width", "height")]
        for px, py in path_points(paths(root, "visible")[0]):
            self.assertTrue(x <= tx + sx * px <= x + w)
            self.assertTrue(y <= ty + sy * py <= y + h)

    def test_zero_width_and_height_bounds_are_finite(self):
        for curve in (line((0, 0), (0, 10)), line((0, 0), (10, 0)), line((0, 0), (0, 0))):
            with self.subTest(curve=curve):
                svg = scene_svg(scene([curve]), width=1, height=1)
                root = ET.fromstring(svg)
                for node in root.iter():
                    for value in node.attrib.values():
                        self.assertNotRegex(value, r"(?i)(?:^|[\s(,])(?:nan|[+-]?inf)(?:$|[\s),])")

    def test_nonfinite_and_bad_point_values_rejected(self):
        for bad in (float("nan"), float("inf"), -float("inf"), True, None, "bad", "2"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                scene_svg(scene([line(preview_points=[[0, 0], [bad, 2]])]))
        for points in ([], [[0, 0]], [[0, 0, 0], [1, 1]], "points"):
            with self.subTest(points=points), self.assertRaises(ValueError):
                scene_svg(scene([line(preview_points=points)]))

    def test_nonfinite_unused_fields_and_cycles_rejected(self):
        for extra in ({"metadata": {"bad": float("nan")}}, {"metadata": {"bad": object()}}):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                scene_svg(scene(**extra))
        cyclic = scene()
        cyclic["metadata"]["cycle"] = cyclic
        with self.assertRaisesRegex(ValueError, "cyclic"):
            scene_svg(cyclic)

    def test_invalid_size_rejected(self):
        for bad in (0, -1, float("nan"), float("inf"), True, "wide", 1e308):
            for key in ("width", "height"):
                with self.subTest(key=key, bad=bad), self.assertRaises(ValueError):
                    scene_svg(scene(), **{key: bad})

    def test_bad_schema_and_entity_shapes(self):
        bad_scenes = [None, [], {}, {"schema_version": True, "views": []},
                      {"schema_version": 2, "views": []}, {"schema_version": 1, "views": "bad"},
                      {"schema_version": 1, "views": [None]}, scene(name=5), scene(warnings=[42]),
                      scene([None]), scene([{}]), scene([dict(type="LINE", start=[0, 0])]),
                      scene([line(role="hatch")]), scene([line(role="unknown")])]
        for value in bad_scenes:
            with self.subTest(value=value), self.assertRaises(ValueError):
                scene_svg(value)

    def test_bad_analytic_geometry(self):
        bad_curves = [dict(type="CIRCLE", center=[0, 0], radius=-1),
                      dict(type="CIRCLE", center=[0, 0], radius=float("inf")),
                      dict(type="ARC", center=[0, 0], radius=1, start_angle=0, end_angle=0),
                      dict(type="ARC", center=[0, 0], radius=1, start_angle=0, end_angle=400),
                      dict(type="ELLIPSE", center=[0, 0], major_axis=[0, 0], ratio=0.5),
                      dict(type="ELLIPSE", center=[0, 0], major_axis=[1, 0], ratio=2),
                      dict(type="CIRCLE", center=[1.7e308, 0], radius=1.7e308)]
        for curve in bad_curves:
            with self.subTest(curve=curve), self.assertRaises(ValueError):
                scene_svg(scene([curve]))

    def test_bad_hatch_input(self):
        for changes in ({"loops": []}, {"spacing": 0}, {"angle_deg": float("nan")},
                        {"loops": [loop([(0, 0), (1, 0), (0, 1)], False)]}):
            hatch = {**holed_hatch(), **changes}
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                scene_svg(scene(hatches=[hatch]))


if __name__ == "__main__":
    unittest.main()
