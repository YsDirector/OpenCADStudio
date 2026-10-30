"""FreeCAD-free schema-v1 scene -> native, millimetre DXF R2013.

``write_dxf`` does not approximate output geometry.  Curve dictionaries use the
``type`` discriminator from the IR; standalone curves require a semantic role,
while hatch boundary edges inherit the hatch role and need no role field.
Metadata/warnings default to {} / []; view entities/hatches are required lists.
Periodic splines must already contain a complete DXF control net and expanded
knot vector (this writer never invents poles, knots, or closing segments).

A single view stays in modelspace. Multiple views use unscaled blocks on layer
0, translated into a horizontal row with conservative bounds and a 20 mm gap.
Counts include block members once, but exclude INSERTs and hatch boundary edges
from the entity total; both have separate counts.

The sibling ``name.manifest.json`` is a commit marker containing the DXF hash.
Both payloads are staged, audited, read back and fsynced before publication. A
cooperative exclusive lock serializes writers; no-overwrite publication uses
atomic hard links. Overwrite withdraws the old manifest before replacing DXF,
and restores the old pair on caught failures. Two paths cannot be atomically
renamed together: a killed process/power loss can leave an uncommitted DXF,
staging directory and stale lock, requiring manual recovery. Consumers should
check the manifest hash, not treat DXF existence alone as proof of success.
"""
from __future__ import annotations

from collections import Counter
from datetime import datetime, timezone
import hashlib
from itertools import groupby
import json
import math
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile
from typing import Any

import ezdxf
from ezdxf.math import BSpline

from .model import ExportError, LAYERS, SCHEMA_VERSION

WRITER_VERSION = "1.0.0"
VIEW_GAP_MM = 20.0
JOIN_TOLERANCE_MM = 1e-6
_LENGTH_EPS = 1e-9
_CURVE_TYPES = {"LINE", "CIRCLE", "ARC", "ELLIPSE", "SPLINE"}
_CURVE_ROLES = set(LAYERS) - {"hatch"}
# 预制图案：自带 45° 斜线，HATCH 角度只用于 0/90 的零件区分。
_HATCH_PATTERN = "ANSI31"
# Complete acadiso-style half-size patterns in millimetres, not empty names.
_LINETYPES = {
    "CENTER2": ("Center (.5x)", [25.4, 15.875, -3.175, 3.175, -3.175]),
    "DASHED2": ("Dashed (.5x)", [7.62, 6.35, -1.27]),
}


def _required(obj: dict, key: str, label: str) -> Any:
    if key not in obj:
        raise ValueError(f"{label}.{key} is required")
    return obj[key]


def _object(value: Any, label: str) -> dict:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    return value


def _array(value: Any, label: str, *, nonempty: bool = False) -> list:
    if not isinstance(value, list) or (nonempty and not value):
        raise ValueError(f"{label} must be a {'nonempty ' if nonempty else ''}list")
    return value


def _number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{label} must be a finite number")
    try:
        result = float(value)
    except OverflowError as exc:
        raise ValueError(f"{label} must be a finite number") from exc
    if not math.isfinite(result):
        raise ValueError(f"{label} must be a finite number")
    return result


def _boolean(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        raise ValueError(f"{label} must be boolean")
    return value


def _xy(value: Any, label: str) -> tuple[float, float]:
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise ValueError(f"{label} must contain exactly two XY coordinates")
    return _number(value[0], label), _number(value[1], label)


def _json_data(value: Any, label: str) -> None:
    """Reject non-JSON metadata and nonfinite values even in extension fields."""
    if value is None or isinstance(value, (str, bool, int)):
        return
    if isinstance(value, float):
        _number(value, label)
        return
    if isinstance(value, (list, tuple)):
        for i, item in enumerate(value):
            _json_data(item, f"{label}[{i}]")
        return
    if isinstance(value, dict) and all(isinstance(k, str) for k in value):
        for key, item in value.items():
            _json_data(item, f"{label}.{key}")
        return
    raise ValueError(f"{label} must be JSON-compatible with string object keys")


def _span(start: float, end: float, period: float, ccw: bool, label: str) -> float:
    delta = end - start
    if not math.isfinite(delta) or abs(delta) > period + 1e-10:
        raise ValueError(f"{label} must span at most one revolution")
    if abs(delta) < 1e-12:
        raise ValueError(f"{label} is degenerate (equal start and end)")
    if math.isclose(abs(delta), period, rel_tol=0, abs_tol=1e-12):
        return period
    result = (delta if ccw else -delta) % period
    if result < 1e-12:
        raise ValueError(f"{label} is degenerate")
    return result


def _interval(start: float, span: float, period: float) -> tuple[float, float]:
    start %= period
    end = start + span
    if span != period:
        end %= period
    return start, end


def _spline_tool(curve: dict) -> BSpline:
    return BSpline(curve["control_points"], order=curve["degree"] + 1,
                   knots=curve["knots"], weights=curve.get("weights"))


def _sample(curve: dict) -> list[tuple[float, float]]:
    kind = curve["type"]
    if kind == "LINE":
        return [curve["start"], curve["end"]]
    if kind == "SPLINE":
        tool = _spline_tool(curve)
        knots = tool.knots()
        domain = sorted(set(knots[curve["degree"]:len(curve["control_points"]) + 1]))
        params = [a + (b - a) * i / 16 for a, b in zip(domain, domain[1:])
                  for i in range(16)] + [domain[-1]]
        points = [tuple(tool.point(t))[:2] for t in params]
    else:
        if kind == "CIRCLE":
            start, span = 0.0, math.tau
        elif kind == "ARC":
            start, span = math.radians(curve["start_angle"]), math.radians(curve["_span"])
        else:
            start, span = curve["start_param"], curve["_span"]
        sign = 1 if curve["ccw"] else -1
        count = max(4, math.ceil(96 * span / math.tau))
        params = [start + sign * span * i / count for i in range(count + 1)]
        cx, cy = curve["center"]
        if kind == "ELLIPSE":
            ax, ay = curve["major_axis"]
            ratio = curve["ratio"]
            points = [(cx + ax * math.cos(t) - ay * ratio * math.sin(t),
                       cy + ay * math.cos(t) + ax * ratio * math.sin(t)) for t in params]
        else:
            radius = curve["radius"]
            points = [(cx + radius * math.cos(t), cy + radius * math.sin(t)) for t in params]
    if not all(math.isfinite(v) for point in points for v in point):
        raise ValueError("curve evaluation produced nonfinite coordinates")
    return points


def _bounds(curves: list[dict]) -> list[list[float]]:
    points = []
    for curve in curves:
        kind = curve["type"]
        if kind == "LINE":
            points.extend((curve["start"], curve["end"]))
        elif kind == "SPLINE":
            # Positive rational weights keep the curve inside its control hull.
            points.extend(curve["control_points"])
        else:
            cx, cy = curve["center"]
            if kind == "ELLIPSE":
                ax, ay = curve["major_axis"]
                ratio = curve["ratio"]
                rx, ry = math.hypot(ax, ay * ratio), math.hypot(ay, ax * ratio)
            else:
                rx = ry = curve["radius"]
            points.extend(((cx - rx, cy - ry), (cx + rx, cy + ry)))
    if not points or not all(math.isfinite(v) for p in points for v in p):
        raise ValueError("view/curve bounds are empty or nonfinite")
    return [[min(p[0] for p in points), min(p[1] for p in points)],
            [max(p[0] for p in points), max(p[1] for p in points)]]


def _curve(value: Any, label: str, *, boundary: bool = False) -> dict:
    obj = _object(value, label)
    kind = _required(obj, "type", label)
    if not isinstance(kind, str) or kind not in _CURVE_TYPES:
        raise ValueError(f"{label}: unsupported geometry type {kind!r}")
    role = obj.get("role") if boundary else _required(obj, "role", label)
    roles = set(LAYERS) if boundary else _CURVE_ROLES
    if ("role" in obj or not boundary) and (not isinstance(role, str) or role not in roles):
        raise ValueError(f"{label}: unknown curve role {role!r}")
    if kind in ("LINE", "SPLINE") and "ccw" in obj:
        raise ValueError(f"{label}: {kind} direction must be encoded by point order, not ccw")
    result = {"type": kind, "role": role}
    number = lambda key: _number(_required(obj, key, label), f"{label}.{key}")
    xy = lambda key: _xy(_required(obj, key, label), f"{label}.{key}")
    if kind == "LINE":
        result.update(start=xy("start"), end=xy("end"))
        length = math.dist(result["start"], result["end"])
        if not math.isfinite(length) or length <= _LENGTH_EPS:
            raise ValueError(f"{label}: degenerate/nonfinite LINE length")
    elif kind in ("CIRCLE", "ARC", "ELLIPSE"):
        result.update(center=xy("center"), ccw=_boolean(obj.get("ccw", True), f"{label}.ccw"))
        if kind in ("CIRCLE", "ARC"):
            result["radius"] = number("radius")
            if result["radius"] <= _LENGTH_EPS:
                raise ValueError(f"{label}: radius must be positive and nondegenerate")
        else:
            result.update(major_axis=xy("major_axis"), ratio=number("ratio"))
            if not 1e-6 <= result["ratio"] <= 1:
                raise ValueError(f"{label}: ELLIPSE ratio must be in [1e-6, 1]")
            if math.hypot(*result["major_axis"]) * result["ratio"] <= _LENGTH_EPS:
                raise ValueError(f"{label}: degenerate ELLIPSE axis")
        if kind != "CIRCLE":
            a, b, period = (("start_angle", "end_angle", 360.0) if kind == "ARC"
                            else ("start_param", "end_param", math.tau))
            result[a], result[b] = number(a), number(b)
            result["_span"] = _span(result[a], result[b], period, result["ccw"], label)
            result[a] %= period
    else:
        degree = _required(obj, "degree", label)
        if isinstance(degree, bool) or not isinstance(degree, int) or not 1 <= degree <= 11:
            raise ValueError(f"{label}: SPLINE degree must be an integer in [1, 11]")
        poles = [_xy(p, f"{label}.control_points[{i}]") for i, p in enumerate(
            _array(_required(obj, "control_points", label), f"{label}.control_points", nonempty=True))]
        knots = [_number(k, f"{label}.knots") for k in
                 _array(_required(obj, "knots", label), f"{label}.knots", nonempty=True)]
        if len(poles) < degree + 1 or len(knots) != len(poles) + degree + 1:
            raise ValueError(f"{label}: invalid SPLINE pole count / expanded knot count")
        if any(a > b for a, b in zip(knots, knots[1:])) or knots[degree] >= knots[len(poles)]:
            raise ValueError(f"{label}: knots must be nondecreasing with a nonzero active domain")
        for knot, group in groupby(knots):
            limit = degree if knots[degree] < knot < knots[len(poles)] else degree + 1
            if len(list(group)) > limit:
                raise ValueError(f"{label}: invalid knot multiplicity / disconnected SPLINE")
        closed = _boolean(obj.get("closed", False), f"{label}.closed")
        periodic = _boolean(obj.get("periodic", False), f"{label}.periodic")
        result.update(degree=degree, control_points=poles, knots=knots,
                      closed=closed or periodic, periodic=periodic)
        if "weights" in obj:
            weights = [_number(w, f"{label}.weights") for w in _array(obj["weights"], f"{label}.weights")]
            if len(weights) != len(poles) or any(w <= 0 for w in weights):
                raise ValueError(f"{label}: weights must be positive, one per control point")
            result["weights"] = weights
        try:
            samples = _sample(result)
        except (ValueError, ZeroDivisionError, OverflowError) as exc:
            raise ValueError(f"{label}: invalid SPLINE evaluation: {exc}") from exc
        if max(math.dist(samples[0], p) for p in samples) <= _LENGTH_EPS:
            raise ValueError(f"{label}: degenerate SPLINE")
        if result["closed"] and math.dist(samples[0], samples[-1]) > JOIN_TOLERANCE_MM:
            raise ValueError(f"{label}: closed/periodic SPLINE requires a geometrically closed control net")
    _bounds([result])
    return result


def _hatch(value: Any, label: str) -> dict:
    obj = _object(value, label)
    if obj.get("role", "hatch") != "hatch" or obj.get("type", "HATCH") != "HATCH":
        raise ValueError(f"{label}: expected HATCH with role 'hatch'")
    angle = _number(_required(obj, "angle_deg", label), f"{label}.angle_deg") % 360
    spacing = _number(_required(obj, "spacing", label), f"{label}.spacing")
    if spacing <= _LENGTH_EPS:
        raise ValueError(f"{label}: spacing must be positive and nondegenerate")
    scale = _number(obj.get("scale", 1.0), f"{label}.scale")
    if scale <= _LENGTH_EPS:
        raise ValueError(f"{label}: scale must be positive and nondegenerate")
    # 预制图案自带斜线；角度只做 0/90 的零件区分，其它值吸附（旧场景 JSON 仍可用）。
    angle = 0.0 if (angle % 180.0) < 45.0 else 90.0
    loops = []
    for i, value in enumerate(_array(_required(obj, "loops", label), f"{label}.loops", nonempty=True)):
        tag = f"{label}.loops[{i}]"
        loop = _object(value, tag)
        outer = _boolean(_required(loop, "is_outer", tag), f"{tag}.is_outer")
        edges = [_curve(e, f"{tag}.edges[{j}]", boundary=True) for j, e in enumerate(
            _array(_required(loop, "edges", tag), f"{tag}.edges", nonempty=True))]
        samples = [_sample(edge) for edge in edges]
        for current, following in zip(samples, samples[1:] + samples[:1]):
            if math.dist(current[-1], following[0]) > JOIN_TOLERANCE_MM:
                raise ValueError(f"{tag}: directed edges do not form a closed loop")
        # Sampling is only a degeneracy check; all exported edges remain native.
        points = [p for edge in samples for p in edge[:-1]]
        ox, oy = points[0]
        area2 = math.fsum((a[0] - ox) * (b[1] - oy) - (b[0] - ox) * (a[1] - oy)
                          for a, b in zip(points, points[1:] + points[:1]))
        if not math.isfinite(area2) or abs(area2) <= _LENGTH_EPS ** 2:
            raise ValueError(f"{tag}: degenerate hatch loop")
        loops.append({"is_outer": outer, "edges": edges})
    if not any(loop["is_outer"] for loop in loops):
        raise ValueError(f"{label}: at least one outer loop is required")
    return {"loops": loops, "angle_deg": angle, "spacing": spacing, "scale": scale}


def _validate(scene: Any) -> dict:
    scene = _object(scene, "scene")
    _json_data(scene, "scene")
    version = _required(scene, "schema_version", "scene")
    if type(version) is not int or version != SCHEMA_VERSION:
        raise ValueError(f"unsupported schema_version {version!r}; expected {SCHEMA_VERSION}")
    metadata = _object(scene.get("metadata", {}), "scene.metadata")
    warnings = _array(scene.get("warnings", []), "scene.warnings")
    if any(not isinstance(w, str) for w in warnings):
        raise ValueError("scene.warnings must contain strings")
    views, names = [], set()
    for i, value in enumerate(_array(_required(scene, "views", "scene"), "scene.views", nonempty=True)):
        tag = f"scene.views[{i}]"
        view = _object(value, tag)
        name = _required(view, "name", tag)
        if not isinstance(name, str) or not name.strip() or name in names:
            raise ValueError(f"{tag}.name must be nonempty and unique")
        names.add(name)
        entities = [_curve(e, f"{tag}.entities[{j}]") for j, e in enumerate(
            _array(_required(view, "entities", tag), f"{tag}.entities"))]
        hatches = [_hatch(h, f"{tag}.hatches[{j}]") for j, h in enumerate(
            _array(_required(view, "hatches", tag), f"{tag}.hatches"))]
        if not entities and not hatches:
            raise ValueError(f"{tag}: empty views cannot be exported")
        all_curves = entities + [e for h in hatches for loop in h["loops"] for e in loop["edges"]]
        views.append({"name": name, "entities": entities, "hatches": hatches,
                      "metadata": _object(view.get("metadata", {}), f"{tag}.metadata"),
                      "bounds": _bounds(all_curves)})
    return {"views": views, "metadata": metadata, "warnings": list(warnings)}


def _attributes(role: str) -> dict:
    return {"layer": LAYERS[role][0], "color": 256, "linetype": "BYLAYER", "lineweight": -1}


def _add_curve(layout: Any, curve: dict) -> None:
    attrs = _attributes(curve["role"])
    kind = curve["type"]
    if kind == "LINE":
        layout.add_line(curve["start"], curve["end"], dxfattribs=attrs)
    elif kind == "CIRCLE":
        layout.add_circle(curve["center"], curve["radius"], dxfattribs=attrs)
    elif kind == "ARC":
        cx, cy = curve["center"]
        start = curve["start_angle"]
        if not curve["ccw"]:
            # ARC coordinates are OCS. A -Z normal mirrors OCS X, preserving
            # both the ordered world endpoints and clockwise world traversal.
            cx, start = -cx, 180 - start
            attrs["extrusion"] = (0, 0, -1)
        start, end = _interval(start, curve["_span"], 360.0)
        layout.add_arc((cx, cy), curve["radius"], start, end, dxfattribs=attrs)
    elif kind == "ELLIPSE":
        start = curve["start_param"]
        if not curve["ccw"]:
            # Unlike ARC, ELLIPSE center and major axis are WCS values.
            start = -start
            attrs["extrusion"] = (0, 0, -1)
        start, end = _interval(start, curve["_span"], math.tau)
        layout.add_ellipse(curve["center"], curve["major_axis"], curve["ratio"],
                           start, end, dxfattribs=attrs)
    else:
        spline = layout.add_spline(degree=curve["degree"], dxfattribs=attrs)
        spline.control_points = curve["control_points"]
        spline.knots = curve["knots"]
        if "weights" in curve:
            spline.weights = curve["weights"]
        spline.dxf.flags = (int(curve["closed"]) | (int(curve["periodic"]) << 1)
                            | (int("weights" in curve) << 2) | 8)  # planar
        spline.dxf.extrusion = (0, 0, 1)


def _add_edge(path: Any, curve: dict) -> None:
    kind = curve["type"]
    if kind == "LINE":
        path.add_line(curve["start"], curve["end"])
    elif kind in ("CIRCLE", "ARC"):
        ccw = curve["ccw"]
        start = 0.0 if kind == "CIRCLE" else curve["start_angle"]
        span = 360.0 if kind == "CIRCLE" else curve["_span"]
        a, b = _interval(start if ccw else start - span, span, 360.0)
        # ezdxf edge angles are always CCW; its exporter handles the DXF
        # complementary-angle convention when the traversal flag is clockwise.
        path.add_arc(curve["center"], curve["radius"], a, b, ccw=ccw)
    elif kind == "ELLIPSE":
        ccw, span = curve["ccw"], curve["_span"]
        start = curve["start_param"]
        a, b = _interval(start if ccw else start - span, span, math.tau)
        edge = path.add_ellipse(curve["center"], curve["major_axis"], curve["ratio"], ccw=ccw)
        # HATCH stores geometric angles, not ellipse parameters in degrees.
        if span == math.tau:
            edge.start_angle, edge.end_angle = 0.0, 360.0
        else:
            edge.start_param, edge.end_param = a, b
            edge.start_angle %= 360.0
            edge.end_angle %= 360.0
    else:
        path.add_spline(control_points=curve["control_points"], knot_values=curve["knots"],
                        weights=curve.get("weights"), degree=curve["degree"],
                        periodic=int(curve["periodic"]))


def _add_hatch(layout: Any, value: dict) -> None:
    hatch = layout.add_hatch(dxfattribs=_attributes("hatch"))
    # 预制图案 ANSI31 已自带 45° 斜线；HATCH 角度只做 0/90 的零件区分。
    # 以前把自定义定义角与 pattern_angle 同时设成 45/135，等于转两次，
    # 导致图案角度异常；现在只交给 set_pattern_fill 旋转一次。
    angle = float(value["angle_deg"]) % 360.0
    scale = float(value.get("scale") or 1.0)
    hatch.set_pattern_fill(
        _HATCH_PATTERN,
        color=256,                      # ByLayer，跟 5剖面线层 颜色
        angle=angle,
        scale=scale,
        double=0,
        style=0,
        pattern_type=1,
        definition=[(45.0, (0.0, 0.0), (-2.2450640303, 2.2450640303), [])],
    )
    for loop in value["loops"]:
        path = hatch.paths.add_edge_path(flags=1 if loop["is_outer"] else 0)
        for edge in loop["edges"]:
            _add_edge(path, edge)


def _layout_translations(views: list[dict], layout: str) -> list[list[float]]:
    """First/third-angle placement: front is the anchor; other views align.

    Variable-size views are placed by their centres, 20 mm apart, then the
    whole sheet is translated so its lower-left bounds are (0, 0).
    """
    gap = VIEW_GAP_MM
    names = [str(view["name"]).strip().lower() for view in views]
    sizes = []
    for view in views:
        (x0, y0), (x1, y1) = view["bounds"]
        sizes.append((x1 - x0, y1 - y0, (x0 + x1) / 2.0, (y0 + y1) / 2.0))
    anchor = names.index("front") if "front" in names else 0
    centres: dict[int, tuple[float, float]] = {}
    rects: list[tuple[float, float, float, float]] = []

    def rect(index, cx, cy):
        w, h = sizes[index][0], sizes[index][1]
        return (cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)

    def overlaps(box):
        return any(not (box[2] + gap <= other[0] or other[2] + gap <= box[0]
                        or box[3] + gap <= other[1] or other[3] + gap <= box[1])
                   for other in rects)

    def put(index, cx, cy):
        box = rect(index, cx, cy)
        while overlaps(box):
            cx += sizes[index][0] + gap
            box = rect(index, cx, cy)
        centres[index] = (cx, cy)
        rects.append(box)

    aw, ah = sizes[anchor][0], sizes[anchor][1]
    put(anchor, 0.0, 0.0)
    sign_top = -1.0 if layout == "first_angle" else 1.0
    sign_left = 1.0 if layout == "first_angle" else -1.0
    standard = {
        "top": (0.0, sign_top),
        "bottom": (0.0, -sign_top),
        "left": (sign_left, 0.0),
        "right": (-sign_left, 0.0),
    }
    remaining = [i for i in range(len(views)) if i != anchor]
    pending = []
    for index in remaining:
        name = names[index]
        w, h = sizes[index][0], sizes[index][1]
        if name in standard:
            dx, dy = standard[name]
            cx = dx * (aw / 2 + gap + w / 2) if dx else 0.0
            cy = dy * (ah / 2 + gap + h / 2) if dy else 0.0
            put(index, cx, cy)
        else:
            pending.append(index)
    for index in pending:
        w, h = sizes[index][0], sizes[index][1]
        right = max(box[2] for box in rects)
        put(index, right + gap + w / 2, 0.0)
    min_x = min(cx - sizes[i][0] / 2 for i, (cx, cy) in centres.items())
    min_y = min(cy - sizes[i][1] / 2 for i, (cx, cy) in centres.items())
    result = []
    for i in range(len(views)):
        cx, cy = centres[i]
        local_cx, local_cy = sizes[i][2], sizes[i][3]
        result.append([cx - local_cx - min_x, cy - local_cy - min_y, 0.0])
    return result


def _build(scene: dict) -> tuple[Any, list[dict]]:
    doc = ezdxf.new("R2013", units=4)
    for key, value in {"$MEASUREMENT": 1, "$LUNITS": 2, "$LUPREC": 4,
                       "$INSBASE": (0, 0, 0), "$LTSCALE": 1.0, "$CELTSCALE": 1.0,
                       "$DIMSCALE": 1.0, "$TILEMODE": 1}.items():
        doc.header[key] = value
    for name, (description, pattern) in _LINETYPES.items():
        doc.linetypes.new(name, dxfattribs={"description": description, "pattern": pattern})
    for name, color, linetype, lineweight in LAYERS.values():
        doc.layers.new(name, dxfattribs={"color": color, "linetype": linetype, "lineweight": lineweight})
    placements = []
    multi = len(scene["views"]) > 1
    layout = str((scene.get("metadata") or {}).get("layout", "first_angle"))
    if layout not in ("first_angle", "third_angle"):
        raise ValueError("metadata.layout must be first_angle or third_angle")
    translations = _layout_translations(scene["views"], layout) if multi else [[0.0, 0.0, 0.0]]
    for i, (view, translation) in enumerate(zip(scene["views"], translations), 1):
        block_name = None
        bounds = view["bounds"]
        if multi:
            block_name = f"OCSM_VIEW_{i:03d}"
            layout_block = doc.blocks.new(block_name)
            layout_block.units = 4
            if not all(math.isfinite(n) for n in translation):
                raise ValueError("multi-view placement overflow")
            doc.modelspace().add_blockref(block_name, translation, dxfattribs={
                "layer": "0", "color": 256, "linetype": "BYLAYER", "lineweight": -1})
        else:
            layout_block = doc.modelspace()
        for curve in view["entities"]:
            _add_curve(layout_block, curve)
        for hatch in view["hatches"]:
            _add_hatch(layout_block, hatch)
        world_bounds = [[p[0] + translation[0], p[1] + translation[1]] for p in bounds]
        if not all(math.isfinite(v) for p in world_bounds for v in p):
            raise ValueError("nonfinite placed bounds")
        placements.append({"view": view["name"], "block": block_name, "translation": translation,
                           "local_bounds_mm": bounds, "bounds_mm": world_bounds})
    return doc, placements


def _audit(doc: Any) -> None:
    audit = doc.audit()
    if audit.errors or audit.fixes:
        messages = [str(item.message) for item in list(audit.errors) + list(audit.fixes)]
        raise ExportError("ezdxf audit rejected errors/repairs: " + "; ".join(messages))


def _inventory(doc: Any, placements: list[dict]) -> dict:
    types, layers, edges = Counter(), Counter({v[0]: 0 for v in LAYERS.values()}), Counter()
    loops = 0
    inserts = list(doc.modelspace().query("INSERT"))
    blocks = sum(p["block"] is not None for p in placements)
    if len(inserts) != blocks or (blocks and len(doc.modelspace()) != blocks):
        raise ExportError("readback: unexpected modelspace/INSERT counts")
    for insert, placement in zip(inserts, placements if blocks else []):
        if (insert.dxf.layer != "0" or insert.dxf.name != placement["block"]
                or tuple(insert.dxf.insert) != tuple(placement["translation"])
                or any(insert.dxf.get(k, 1) != 1 for k in ("xscale", "yscale", "zscale"))
                or insert.dxf.get("rotation", 0) != 0):
            raise ExportError("readback: block placement/scaling changed")
        _check_bylayer(insert)
    for placement in placements:
        layout = doc.blocks.get(placement["block"]) if placement["block"] else doc.modelspace()
        if placement["block"] and layout.units != 4:
            raise ExportError("readback: block units changed")
        for entity in layout:
            kind, layer = entity.dxftype(), entity.dxf.layer
            if kind not in _CURVE_TYPES | {"HATCH"} or layer not in layers:
                raise ExportError(f"readback: unexpected entity/layer {kind}/{layer}")
            _check_bylayer(entity)
            types[kind] += 1
            layers[layer] += 1
            if kind == "HATCH":
                loops += len(entity.paths)
                for path in entity.paths:
                    for edge in path.edges:
                        edges[type(edge).__name__] += 1
    return {"entities": sum(types.values()), "by_type": dict(sorted(types.items())),
            "by_layer": dict(layers), "by_role": {r: layers[v[0]] for r, v in LAYERS.items()},
            "views": len(placements), "blocks": blocks, "inserts": len(inserts),
            "hatch_loops": loops, "hatch_edges": sum(edges.values()),
            "hatch_edges_by_type": dict(sorted(edges.items()))}


def _check_bylayer(entity: Any) -> None:
    if (entity.dxf.get("color", 256) != 256
            or entity.dxf.get("linetype", "BYLAYER").upper() != "BYLAYER"
            or entity.dxf.get("lineweight", -1) != -1
            or entity.dxf.get("ltscale", 1) != 1
            or entity.dxf.hasattr("true_color")):
        raise ExportError(f"readback: {entity.dxftype()} is not entirely ByLayer")


def _check_document(doc: Any) -> None:
    if (doc.dxfversion != "AC1027" or doc.units != 4 or doc.output_encoding != "utf-8"
            or doc.header["$MEASUREMENT"] != 1 or doc.header["$DIMSCALE"] != 1
            or doc.header["$LTSCALE"] != 1):
        raise ExportError("readback: version/encoding/mm/1:1 settings changed")
    for name, color, linetype, weight in LAYERS.values():
        layer = doc.layers.get(name)
        if (layer.dxf.color, layer.dxf.linetype, layer.dxf.lineweight) != (color, linetype, weight):
            raise ExportError(f"readback: layer definition changed: {name}")
    for name, (_, pattern) in _LINETYPES.items():
        tags = doc.linetypes.get(name).pattern_tags.tags
        expected_tags = {72: [65], 73: [len(pattern) - 1], 40: [pattern[0]], 49: pattern[1:]}
        if any([tag.value for tag in tags if tag.code == code] != values
               for code, values in expected_tags.items()):
            raise ExportError(f"readback: incomplete linetype: {name}")


def _sync_directory(path: Path) -> None:
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _check_targets(paths: tuple[Path, Path], overwrite: bool) -> None:
    for path in paths:
        if os.path.lexists(path):
            if not overwrite:
                raise FileExistsError(f"target exists (use overwrite=True): {path}")
            if not stat.S_ISREG(path.lstat().st_mode):
                raise ValueError(f"refusing to overwrite non-regular file or symlink: {path}")


class _RollbackError(ExportError):
    """Keep staging and the lock when manual recovery is necessary."""


def _publish(staged: tuple[Path, Path], targets: tuple[Path, Path], overwrite: bool) -> None:
    dxf, manifest = targets
    backup_dxf = staged[0].parent / "previous.dxf"
    backup_manifest = staged[0].parent / "previous.manifest.json"
    installed_dxf = installed_manifest = withdrawn_manifest = False
    try:
        if overwrite:
            if dxf.exists():
                os.link(dxf, backup_dxf)
            if manifest.exists():
                os.replace(manifest, backup_manifest)
                withdrawn_manifest = True
                # Make withdrawal durable before the DXF can change.
                _sync_directory(dxf.parent)
            _sync_directory(staged[0].parent)
            os.replace(staged[0], dxf)
        else:
            os.link(staged[0], dxf)  # atomic, no-clobber even against external writers
        installed_dxf = True
        # The old manifest is absent. No-clobber protects an unexpected new one.
        os.link(staged[1], manifest)
        installed_manifest = True
        _sync_directory(dxf.parent)
    except BaseException as exc:
        try:
            if installed_manifest:
                manifest.unlink()
            if installed_dxf:
                if backup_dxf.exists():
                    os.replace(backup_dxf, dxf)
                else:
                    dxf.unlink()
            if withdrawn_manifest:
                os.replace(backup_manifest, manifest)
            _sync_directory(dxf.parent)
        except BaseException as rollback_exc:
            raise _RollbackError(
                f"publication failed ({exc}); rollback failed ({rollback_exc}); "
                f"recovery files retained in {staged[0].parent}") from exc
        raise


def write_dxf(scene: dict, path: str | Path, *, overwrite: bool = False) -> dict:
    """Validate and atomically publish DXF + manifest; return a JSON report.

    Raises ValueError for invalid IR, FileExistsError for either occupied target
    (or a writer lock), OSError for filesystem failures, ExportError for audit /
    readback failures. No caught failure is reported as a successful export.
    The output directory must already exist. Existing target symlinks are never
    followed or overwritten. ``overwrite`` must be an explicit bool.
    """
    if not isinstance(overwrite, bool):
        raise ValueError("overwrite must be boolean")
    validated = _validate(scene)
    path = Path(path).expanduser().absolute()
    if path.suffix.lower() != ".dxf":
        raise ValueError("DXF output path must end in .dxf")
    path = path.parent.resolve(strict=True) / path.name
    manifest_path = path.with_suffix(".manifest.json")
    if path == manifest_path or path.name.endswith(".manifest.json"):
        raise ValueError("DXF output must not itself be a .manifest.json sidecar")
    targets = (path, manifest_path)
    _check_targets(targets, overwrite)
    doc, placements = _build(validated)
    expected = _inventory(doc, placements)
    _audit(doc)
    if _inventory(doc, placements) != expected:
        raise ExportError("audit changed geometry counts")
    lock = path.with_name(f".{path.name}.writer.lock")
    fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    stage = None
    keep_recovery = published = False
    report = None
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(f"pid={os.getpid()}\n")
            stream.flush()
            os.fsync(stream.fileno())
        _check_targets(targets, overwrite)
        stage = Path(tempfile.mkdtemp(prefix=f".{path.name}.stage-", dir=path.parent))
        staged_dxf, staged_manifest = stage / "payload.dxf", stage / "payload.manifest.json"
        with staged_dxf.open("w", encoding="utf-8", errors="strict", newline="\n") as stream:
            doc.write(stream)
            stream.flush()
            os.fsync(stream.fileno())
        readback = ezdxf.readfile(staged_dxf)
        _audit(readback)
        _check_document(readback)
        counts = _inventory(readback, placements)
        if counts != expected:
            raise ExportError("DXF readback entity/layer/boundary counts differ from input")
        with staged_dxf.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        metadata = validated["metadata"]
        report = {"path": str(path), "manifest_path": str(manifest_path),
                  "counts": counts, "warnings": validated["warnings"], "placements": placements,
                  "sha256": digest}
        manifest = {**report, "status": "complete", "schema_version": SCHEMA_VERSION,
                    "writer": {"name": "ocsm_export.dxf_writer", "version": WRITER_VERSION,
                               "ezdxf_version": ezdxf.__version__},
                    "created_utc": datetime.now(timezone.utc).isoformat(),
                    "source": metadata.get("source", metadata.get("sources", metadata.get("source_path"))),
                    "settings": metadata.get("settings", metadata.get("options", {})),
                    "metadata": metadata,
                    "views": [{"name": v["name"], "metadata": v["metadata"]} for v in validated["views"]],
                    "scene_sha256": hashlib.sha256(json.dumps(scene, ensure_ascii=False, sort_keys=True,
                                                               allow_nan=False).encode("utf-8")).hexdigest(),
                    "dxf": {"version": "AC1027", "release": "R2013", "encoding": "UTF-8",
                            "units": "mm", "model_scale": 1, "size_bytes": staged_dxf.stat().st_size},
                    "audit": {"errors": 0, "fixes": 0, "readback_counts_verified": True}}
        with staged_manifest.open("w", encoding="utf-8", newline="\n") as stream:
            json.dump(manifest, stream, ensure_ascii=False, indent=2, allow_nan=False)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        _publish((staged_dxf, staged_manifest), targets, overwrite)
        published = True
        return report
    except _RollbackError:
        keep_recovery = True
        raise
    finally:
        if not keep_recovery:
            pending_error = sys.exception()
            cleanup_errors = []
            if stage is not None:
                try:
                    shutil.rmtree(stage)
                except OSError as exc:
                    cleanup_errors.append(f"could not remove staging directory {stage}: {exc}")
            try:
                lock.unlink(missing_ok=True)
            except OSError as exc:
                cleanup_errors.append(f"could not remove writer lock {lock}: {exc}")
            # Never report a committed, valid pair as a failed export merely
            # because transient-file cleanup failed. These runtime warnings
            # are in the report only, since the manifest is already committed.
            for message in cleanup_errors:
                if published:
                    report["warnings"].append("Export committed; " + message)
                elif pending_error is not None:
                    pending_error.add_note(message)
