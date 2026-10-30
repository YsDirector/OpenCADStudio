"""Read-only, stdlib-only SVG preview of the schema_version=1 scene.

Samples are display geometry, never a replacement for the exact DXF curves.
Each view is fitted independently. Dimensions describe sampled bounds in mm.
"""
from __future__ import annotations

from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from html import escape
import math
from typing import Any

from .model import LAYERS, SCHEMA_VERSION

Point = tuple[float, float]
TAU = 2 * math.pi


def _number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{label} must be a finite number")
    try:
        result = float(value)
    except (TypeError, ValueError, OverflowError) as exc:
        raise ValueError(f"{label} must be a finite number") from exc
    if not math.isfinite(result):
        raise ValueError(f"{label} must be a finite number")
    return result


def _list(value: Any, label: str) -> Sequence:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes, bytearray)):
        raise ValueError(f"{label} must be a list")
    return value


def _mapping(value: Any, label: str) -> Mapping:
    if not isinstance(value, Mapping):
        raise ValueError(f"{label} must be an object")
    return value


def _point(value: Any, label: str) -> Point:
    value = _list(value, label)
    if len(value) != 2:
        raise ValueError(f"{label} must contain two finite numbers")
    return (_number(value[0], label), _number(value[1], label))


def _boolean(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        raise ValueError(f"{label} must be boolean")
    return value


def _fmt(value: float) -> str:
    return format(_number(value, "SVG coordinate"), ".10g")


def _text(value: Any) -> str:
    # XML 1.0 excludes control characters even when HTML-escaped.
    text = str(value)
    text = "".join(c if (c in "\t\n\r" or 0x20 <= ord(c) <= 0xD7FF
                          or 0xE000 <= ord(c) <= 0xFFFD
                          or 0x10000 <= ord(c) <= 0x10FFFF) else "\ufffd" for c in text)
    return escape(text, quote=True)


def _sweep(start: float, end: float, ccw: bool, period: float) -> float:
    delta = _number(end - start, "curve interval")
    if abs(delta) < 1e-12 or abs(delta) > period + 1e-10:
        raise ValueError("curve interval must be nonzero and span at most one revolution")
    if math.isclose(abs(delta), period, rel_tol=0, abs_tol=1e-12):
        return period if ccw else -period
    return delta % period if ccw else -((-delta) % period)


@dataclass
class _Curve:
    points: list[Point]
    role: str
    closed: bool = False


def _curve(value: Any, label: str, warnings: list[str], *, boundary: bool = False) -> _Curve | None:
    curve = _mapping(value, label)
    kind = curve.get("type")
    if not isinstance(kind, str) or not kind:
        raise ValueError(f"{label}.type must be a nonempty string")
    kind = kind.upper()
    role = curve.get("role", "hatch" if boundary else "visible")
    roles = LAYERS if boundary else ("visible", "hidden", "thin", "center")
    if not isinstance(role, str) or role not in roles:
        raise ValueError(f"{label}.role is not a supported engineering role")
    closed = _boolean(curve.get("closed", False), f"{label}.closed") or kind == "CIRCLE"
    if curve.get("preview_points") is not None:
        points = [_point(p, f"{label}.preview_points[{i}]")
                  for i, p in enumerate(_list(curve["preview_points"], f"{label}.preview_points"))]
        if len(points) < 2:
            raise ValueError(f"{label}.preview_points requires at least two points")
        return _Curve(points, role, closed)
    if kind == "LINE":
        return _Curve([_point(curve.get("start"), f"{label}.start"),
                       _point(curve.get("end"), f"{label}.end")], role)
    if kind not in ("CIRCLE", "ARC", "ELLIPSE"):
        warnings.append(f"{label}: {kind} 缺少 preview_points，无法预览；未绘制控制多边形。")
        return None
    center = _point(curve.get("center"), f"{label}.center")
    ccw = _boolean(curve.get("ccw", True), f"{label}.ccw")
    if kind == "ELLIPSE":
        major = _point(curve.get("major_axis"), f"{label}.major_axis")
        radius = math.hypot(*major)
        ratio = _number(curve.get("ratio"), f"{label}.ratio")
        if not math.isfinite(radius) or radius <= 0 or ratio <= 0 or ratio > 1:
            raise ValueError(f"{label}: ellipse requires a nonzero major_axis and 0 < ratio <= 1")
        start = _number(curve.get("start_param", 0), f"{label}.start_param")
        end = _number(curve.get("end_param", TAU), f"{label}.end_param")
        sweep = _sweep(start, end, ccw, TAU)
        start %= TAU
        minor = (-major[1] * ratio, major[0] * ratio)
        closed = closed or abs(abs(sweep) - TAU) < 1e-10
    else:
        radius = _number(curve.get("radius"), f"{label}.radius")
        if radius <= 0:
            raise ValueError(f"{label}.radius must be positive")
        major, minor = (radius, 0.0), (0.0, radius)
        if kind == "CIRCLE":
            start, sweep = 0.0, TAU if ccw else -TAU
        else:
            start_deg = _number(curve.get("start_angle"), f"{label}.start_angle")
            end_deg = _number(curve.get("end_angle"), f"{label}.end_angle")
            sweep = math.radians(_sweep(start_deg, end_deg, ccw, 360))
            start = math.radians(start_deg % 360)
    count = max(8, math.ceil(abs(sweep) / TAU * 128))
    points = []
    for i in range(count + 1):
        t = start + sweep * i / count
        c, s = math.cos(t), math.sin(t)
        points.append(_point((center[0] + major[0] * c + minor[0] * s,
                              center[1] + major[1] * c + minor[1] * s), label))
    return _Curve(points, role, closed)


@dataclass
class _Hatch:
    loops: list[list[Point]]
    outer_count: int
    angle: float
    spacing: float
    scale: float = 1.0


def _near(a: Point, b: Point) -> bool:
    return math.dist(a, b) <= 1e-6


def _hatch(value: Any, label: str, warnings: list[str]) -> _Hatch | None:
    hatch = _mapping(value, label)
    angle = _number(hatch.get("angle_deg", 0), f"{label}.angle_deg")
    spacing = _number(hatch.get("spacing", 3.175), f"{label}.spacing")
    if spacing <= 0:
        raise ValueError(f"{label}.spacing must be positive")
    scale = _number(hatch.get("scale", 1.0), f"{label}.scale")
    if scale <= 0:
        raise ValueError(f"{label}.scale must be positive")
    raw_loops = _list(hatch.get("loops"), f"{label}.loops")
    if not raw_loops:
        raise ValueError(f"{label}.loops must not be empty")
    loops, outer_count, incomplete = [], 0, False
    for i, raw in enumerate(raw_loops):
        loop = _mapping(raw, f"{label}.loops[{i}]")
        outer_count += _boolean(loop.get("is_outer"), f"{label}.loops[{i}].is_outer")
        edges = _list(loop.get("edges"), f"{label}.loops[{i}].edges")
        if not edges:
            raise ValueError(f"{label}.loops[{i}].edges must not be empty")
        points: list[Point] = []
        for j, edge in enumerate(edges):
            sampled = _curve(edge, f"{label}.loops[{i}].edges[{j}]", warnings, boundary=True)
            if sampled is None:
                incomplete = True
                continue
            edge_points = list(sampled.points)
            if sampled.closed and not _near(edge_points[0], edge_points[-1]):
                edge_points.append(edge_points[0])
            if points and not _near(points[-1], edge_points[0]):
                incomplete = True
            points.extend(edge_points if not points else edge_points[1:])
        if len(points) < 4 or not _near(points[0], points[-1]):
            incomplete = True
        loops.append(points)
    if not outer_count:
        raise ValueError(f"{label} requires an outer loop")
    if incomplete:
        # Never omit just an inner hole and misleadingly fill it in.
        warnings.append(f"{label}: 环不闭合/边界不可预览，整个 HATCH 已省略（保留孔洞安全）。")
        return None
    return _Hatch(loops, outer_count, angle % 360, spacing, scale)


@dataclass
class _View:
    name: str
    curves: list[_Curve] = field(default_factory=list)
    hatches: list[_Hatch] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    layers: Counter = field(default_factory=Counter)
    bounds: tuple[float, float, float, float] | None = None

    def dimensions(self) -> tuple[float, float]:
        if self.bounds is None:
            return 0.0, 0.0
        x0, y0, x1, y1 = self.bounds
        return x1 - x0, y1 - y0


def _json_data(value: Any, active: set[int] | None = None) -> None:
    """Reject nonfinite extension fields too, without evaluating spline controls."""
    if value is None or isinstance(value, (str, bool)):
        return
    if isinstance(value, (int, float)):
        _number(value, "scene numeric value")
        return
    active = set() if active is None else active
    if id(value) in active:
        raise ValueError("scene must not contain cyclic references")
    active.add(id(value))
    try:
        if isinstance(value, Mapping) and all(isinstance(k, str) for k in value):
            items = value.values()
        elif isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
            items = value
        else:
            raise ValueError("scene must be JSON-compatible with string keys")
        for item in items:
            _json_data(item, active)
    finally:
        active.remove(id(value))


def _parse(scene: Any) -> tuple[list[_View], list[str]]:
    scene = _mapping(scene, "scene")
    _json_data(scene)
    version = scene.get("schema_version")
    if type(version) is not int or version != SCHEMA_VERSION:
        raise ValueError(f"scene.schema_version must be {SCHEMA_VERSION}")
    warnings = list(_list(scene.get("warnings", []), "scene.warnings"))
    if any(not isinstance(w, str) for w in warnings):
        raise ValueError("scene.warnings must contain strings")
    views = []
    for i, raw in enumerate(_list(scene.get("views"), "scene.views")):
        raw = _mapping(raw, f"views[{i}]")
        name = raw.get("name", f"view-{i + 1}")
        if not isinstance(name, str):
            raise ValueError(f"views[{i}].name must be a string")
        view = _View(name)
        for j, entity in enumerate(_list(raw.get("entities", []), f"{name}.entities")):
            curve = _curve(entity, f"{name}.entities[{j}]", view.warnings)
            if curve is not None:
                view.curves.append(curve)
                view.layers[curve.role] += 1
        for j, raw_hatch in enumerate(_list(raw.get("hatches", []), f"{name}.hatches")):
            hatch = _hatch(raw_hatch, f"{name}.hatches[{j}]", view.warnings)
            if hatch is not None:
                view.hatches.append(hatch)
                view.layers["hatch"] += 1
        points = [p for c in view.curves for p in c.points]
        points.extend(p for h in view.hatches for loop in h.loops for p in loop)
        if points:
            xs, ys = zip(*points)
            view.bounds = min(xs), min(ys), max(xs), max(ys)
            for size in view.dimensions():
                _number(size, f"{name}: extent (coordinates too large)")
        views.append(view)
    return views, warnings


def _summary(views: list[_View], warnings: list[str]) -> str:
    lines = ["SVG 纯预览；尺寸为采样范围（mm），各视图独立适配，不代表纸面比例。"]
    if not views:
        lines.append("空场景：无视图 / No views")
    for view in views:
        w, h = view.dimensions()
        lines.append(f"{view.name}: {w:.6g} × {h:.6g} mm")
        layers = "; ".join(f"{LAYERS[role][0]}: {view.layers[role]}" for role in LAYERS if view.layers[role])
        lines.append("  " + (layers or "无可预览几何 / No geometry"))
        lines.extend("  警告: " + warning for warning in view.warnings)
    lines.extend("警告: " + warning for warning in warnings)
    return "\n".join(lines)


def scene_summary(scene: dict) -> str:
    """Plain-text dimensions, displayed layers and warnings for the GUI report."""
    return _summary(*_parse(scene))


def _path(points: list[Point], closed: bool = False) -> str:
    return "M " + " L ".join(f"{_fmt(x)} {_fmt(y)}" for x, y in points) + (" Z" if closed else "")


def scene_svg(scene: dict, width: int = 900, height: int = 650) -> str:
    """Return self-contained, escaped SVG; malformed/nonfinite input raises ValueError.

    Unsupported unsampled curves are explicitly marked, not silently drawn as
    control polygons. An incomplete hatch is omitted as a whole, never hole-filled.
    """
    width, height = _number(width, "width"), _number(height, "height")
    if width <= 0 or height <= 0 or width > 1e6 or height > 1e6:
        raise ValueError("width and height must be positive and at most 1000000")
    views, warnings = _parse(scene)
    # Work in a stable minimum logical canvas, including for tiny requested sizes.
    cw, ch = max(width, 400.0), max(height, 300.0)
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{_fmt(width)}" height="{_fmt(height)}" '
             f'viewBox="0 0 {_fmt(cw)} {_fmt(ch)}">',
             '<title>OCSM 只读工程视图预览</title>',
             f'<desc>{_text(_summary(views, warnings))}</desc>',
             '<rect width="100%" height="100%" fill="white"/>']
    if not views:
        parts.append('<text x="20" y="45" font-size="16">空场景：无视图 / No views</text>')
    else:
        cols = min(len(views), max(1, math.ceil(math.sqrt(len(views) * cw / ch))))
        rows = math.ceil(len(views) / cols)
        cell_w, cell_h = (cw - 20) / cols, (ch - 42) / rows
        for index, view in enumerate(views):
            x, y = 10 + index % cols * cell_w, 10 + index // cols * cell_h
            w, h = view.dimensions()
            title = f"{view.name} — {w:.6g} × {h:.6g} mm"
            parts.extend([f'<g id="view-{index}" data-view="{_text(view.name)}">',
                          f'<title>{_text(title)}</title>',
                          f'<rect x="{_fmt(x)}" y="{_fmt(y)}" width="{_fmt(cell_w - 6)}" '
                          f'height="{_fmt(cell_h - 6)}" fill="none" stroke="#c8cdd4"/>',
                          f'<text x="{_fmt(x + 8)}" y="{_fmt(y + 20)}" font-size="13">{_text(title)}</text>'])
            layers = "; ".join(f"{LAYERS[r][0]}:{view.layers[r]}" for r in LAYERS if view.layers[r])
            parts.append(f'<text x="{_fmt(x + 8)}" y="{_fmt(y + 36)}" font-size="9">{_text(layers)}</text>')
            px, py = x + 12, y + 46
            pw, ph = max(1.0, cell_w - 30), max(1.0, cell_h - 84)
            parts.append(f'<defs><clipPath id="clip-{index}"><rect x="{_fmt(px)}" y="{_fmt(py)}" '
                         f'width="{_fmt(pw)}" height="{_fmt(ph)}"/></clipPath></defs>')
            if view.bounds is not None:
                x0, y0, x1, y1 = view.bounds
                scale = min(pw / max(w, 1e-9), ph / max(h, 1e-9)) * 0.94
                tx = px + pw / 2 - (x0 / 2 + x1 / 2) * scale
                ty = py + ph / 2 + (y0 / 2 + y1 / 2) * scale
                parts.append(f'<g clip-path="url(#clip-{index})"><g data-units="mm" '
                             f'transform="translate({_fmt(tx)} {_fmt(ty)}) scale({_fmt(scale)} {_fmt(-scale)})">')
                for j, hatch in enumerate(view.hatches):
                    ident = f"hatch-{index}-{j}"
                    spacing = _fmt(hatch.spacing)
                    parts.extend([f'<defs><pattern id="{ident}" patternUnits="userSpaceOnUse" '
                                  f'width="{spacing}" height="{spacing}" patternTransform="rotate({_fmt(hatch.angle)})">',
                                  f'<path d="M 0 0 L {spacing} 0" fill="none" stroke="#98751b" '
                                  f'stroke-width="{_fmt(0.7 / scale)}"/></pattern></defs>',
                                  f'<path data-role="hatch" data-outer-count="{hatch.outer_count}" '
                                  f'data-inner-count="{len(hatch.loops) - hatch.outer_count}" '
                                  f'd="{" ".join(_path(loop, True) for loop in hatch.loops)}" '
                                  f'fill="url(#{ident})" fill-rule="evenodd" stroke="none"/>'])
                for curve in view.curves:
                    color = {"visible": "#202832", "hidden": "#9b368d", "center": "#b03932", "thin": "#237e85"}[curve.role]
                    dash = {"hidden": (6, 3), "center": (10, 2, 2, 2)}.get(curve.role)
                    dash_attr = (' stroke-dasharray="' + " ".join(_fmt(v / scale) for v in dash) + '"') if dash else ""
                    stroke = (1.5 if curve.role == "visible" else 0.9) / scale
                    parts.append(f'<path data-role="{curve.role}" data-layer="{_text(LAYERS[curve.role][0])}" '
                                 f'd="{_path(curve.points, curve.closed)}" fill="none" stroke="{color}" '
                                 f'stroke-width="{_fmt(stroke)}"{dash_attr}/>')
                parts.append('</g></g>')
            else:
                parts.append(f'<text x="{_fmt(px + 6)}" y="{_fmt(py + 22)}" font-size="12">无可预览几何 / No geometry</text>')
            if view.warnings:
                parts.append(f'<text data-placeholder="true" x="{_fmt(x + 8)}" y="{_fmt(y + cell_h - 17)}" '
                             f'font-size="11" fill="#b42318"><title>{_text(chr(10).join(view.warnings))}</title>'
                             f'⚠ {_text(view.warnings[0][:75])}（共 {len(view.warnings)} 项，详见报告）</text>')
            parts.append('</g>')
    count = len(warnings) + sum(len(v.warnings) for v in views)
    footer = f"采样预览 · mm · 各分区独立适配 · 警告 {count}（详见报告）"
    parts.extend([f'<text x="12" y="{_fmt(ch - 12)}" font-size="11">{_text(footer)}</text>', '</svg>'])
    return "\n".join(parts)
