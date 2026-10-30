"""Lossless OCC-edge to planar curve conversion, including directed hatch loops."""
from __future__ import annotations

import math
from typing import Any, Callable

from .model import ExportError, ExportOptions, Vec3, dot

TAU = 2.0 * math.pi


def _xyz(point: Any) -> Vec3:
    return (float(point.x), float(point.y), float(point.z))


def projector(basis: tuple[Vec3, Vec3, Vec3] | None = None) -> Callable[[Any], list[float]]:
    if basis is None:
        return lambda p: [float(p.x), float(p.y)]
    return lambda p: [dot(_xyz(p), basis[0]), dot(_xyz(p), basis[1])]


def _conic(edge: Any, project: Callable) -> dict:
    """Project a circle/ellipse analytically using the 2x2 covariance matrix.

    This also preserves conics for oblique section planes. OCC's parallel-wire
    projection can approximate circles with high-degree B-splines, so is not used.
    """
    curve = edge.Curve
    circle = type(curve).__name__ == "Circle"
    a = float(curve.Radius if circle else curve.MajorRadius)
    b = float(curve.Radius if circle else curve.MinorRadius)
    center = project(curve.Center)
    u, v = project(curve.XAxis * a), project(curve.YAxis * b)
    xx, yy = u[0] ** 2 + v[0] ** 2, u[1] ** 2 + v[1] ** 2
    xy = u[0] * u[1] + v[0] * v[1]
    disc = math.hypot(xx - yy, 2.0 * xy)
    major = math.sqrt(max(0.0, (xx + yy + disc) / 2.0))
    # det(A)^2 / largest eigenvalue avoids cancellation for eccentric ellipses.
    determinant = u[0] * v[1] - u[1] * v[0]
    minor = abs(determinant) / major if major else 0.0
    if minor < 1e-9:
        raise ExportError("A conic projects to a degenerate line; choose a non-edge-on view.")
    theta = 0.5 * math.atan2(2.0 * xy, xx - yy)
    ux, uy = math.cos(theta), math.sin(theta)
    reverse = edge.Orientation == "Reversed"
    ccw = (determinant > 0) != reverse
    first, last = float(edge.FirstParameter), float(edge.LastParameter)
    start = project(edge.valueAt(last if reverse else first))
    end = project(edge.valueAt(first if reverse else last))
    full = bool(edge.isClosed()) or abs(last - first) >= TAU - 1e-9

    def param(point: list[float]) -> float:
        x, y = point[0] - center[0], point[1] - center[1]
        return math.atan2((-uy * x + ux * y) / minor, (ux * x + uy * y) / major) % TAU

    if abs(major - minor) <= max(major, 1.0) * 1e-10:
        if full:
            return {"type": "CIRCLE", "center": center, "radius": major, "ccw": ccw}
        start_angle = math.degrees(math.atan2(start[1] - center[1], start[0] - center[0])) % 360
        end_angle = math.degrees(math.atan2(end[1] - center[1], end[0] - center[0])) % 360
        return {"type": "ARC", "center": center, "radius": major,
                "start_angle": start_angle, "end_angle": end_angle, "ccw": ccw}
    return {"type": "ELLIPSE", "center": center,
            "major_axis": [major * ux, major * uy], "ratio": minor / major,
            "start_param": 0.0 if full else param(start),
            "end_param": TAU if full else param(end), "ccw": ccw}


def _spline(edge: Any, project: Callable) -> dict:
    curve = edge.Curve
    if type(curve).__name__ == "BSplineCurve":
        spline = curve.copy()
        spline.segment(float(edge.FirstParameter), float(edge.LastParameter))
    else:
        # Bezier and bounded conic -> exact rational B-spline, never a polyline.
        nurbs = edge.toNurbs()
        if len(nurbs.Edges) != 1:
            raise ExportError("Exact NURBS conversion did not produce one edge.")
        spline = nurbs.Edges[0].Curve.copy()
        spline.segment(float(nurbs.Edges[0].FirstParameter), float(nurbs.Edges[0].LastParameter))
    if edge.Orientation == "Reversed":
        spline.reverse()
    knots = [float(k) for k, count in zip(spline.getKnots(), spline.getMultiplicities())
             for _ in range(count)]
    return {"type": "SPLINE", "degree": spline.Degree,
            "control_points": [project(p) for p in spline.getPoles()], "knots": knots,
            "weights": [float(w) for w in spline.getWeights()],
            "periodic": bool(spline.isPeriodic()), "closed": bool(spline.isClosed())}


def _linearized(data: dict, role: str) -> list[dict]:
    """Degree-1 non-rational B-spline -> one LINE or exact LINE segments."""
    points = []
    for point in data["control_points"]:
        if not points or math.dist(point, points[-1]) > 1e-9:
            points.append(point)
    closed = bool(data.get("closed") or data.get("periodic"))
    if closed and math.dist(points[0], points[-1]) > 1e-9:
        points.append(points[0])
    if len(points) < 2:
        raise ExportError("A degree-1 spline edge has no usable control polygon.")
    first, last = points[0], points[-1]
    length = math.dist(first, last)
    if length > 1e-9 and not closed:
        ux, uy = (last[0] - first[0]) / length, (last[1] - first[1]) / length
        offsets = [abs((p[0] - first[0]) * uy - (p[1] - first[1]) * ux) for p in points]
        spans = [(p[0] - first[0]) * ux + (p[1] - first[1]) * uy for p in points]
        if max(offsets) <= 1e-7 and all(a <= b + 1e-9 for a, b in zip(spans, spans[1:])):
            return [{"type": "LINE", "role": role, "start": first, "end": last,
                     "preview_points": [first, last]}]
    result = []
    for start, end in zip(points, points[1:]):
        if math.dist(start, end) >= 1e-9:
            result.append({"type": "LINE", "role": role, "start": start, "end": end,
                           "preview_points": [start, end]})
    if not result:
        raise ExportError("A degree-1 spline collapsed to zero-length segments.")
    return result


def _preview(edge: Any, project: Callable, tolerance: float) -> list[list[float]]:
    if type(edge.Curve).__name__ in ("Line", "LineSegment"):
        points = [edge.valueAt(edge.FirstParameter), edge.valueAt(edge.LastParameter)]
    else:
        points = edge.discretize(Deflection=max(tolerance, 0.02))
        if len(points) > 2000:
            points = edge.discretize(Number=2000)
    if edge.Orientation == "Reversed":
        points.reverse()
    return [project(p) for p in points]


def edge_curves(edge: Any, role: str, options: ExportOptions,
                warnings: list[str], basis: tuple[Vec3, Vec3, Vec3] | None = None) -> list[dict]:
    project = projector(basis)
    if edge.Length < 1e-9:
        warnings.append("Filtered a zero-length projected edge (<1e-9 mm).")
        return []
    kind = type(edge.Curve).__name__
    try:
        if kind in ("Line", "LineSegment"):
            start, end = edge.valueAt(edge.FirstParameter), edge.valueAt(edge.LastParameter)
            if edge.Orientation == "Reversed":
                start, end = end, start
            start, end = project(start), project(end)
            if math.dist(start, end) < 1e-9:
                raise ExportError("An edge is parallel to the view direction and has no planar length.")
            result = {"type": "LINE", "start": start, "end": end}
        elif kind in ("Circle", "Ellipse"):
            result = _conic(edge, project)
        elif kind in ("BSplineCurve", "BezierCurve", "Parabola", "Hyperbola"):
            result = _spline(edge, project)
            if (result["degree"] == 1
                    and all(abs(weight - 1.0) <= 1e-12 for weight in result["weights"])):
                return _linearized(result, role)
        else:
            raise ExportError(f"Unsupported exact curve type: {kind}.")
        result["role"] = role
        result["preview_points"] = _preview(edge, project, options.tolerance)
        return [result]
    except Exception as exc:
        if not options.allow_approximation:
            raise ExportError(f"Cannot export {kind} exactly: {exc}") from exc
        # Explicit opt-in only. The tolerance and source type remain in warnings.
        points = edge.discretize(Deflection=options.tolerance)
        if edge.Orientation == "Reversed":
            points.reverse()
        points = [project(p) for p in points]
        curves = [{"type": "LINE", "role": role, "start": a, "end": b,
                   "preview_points": [a, b]} for a, b in zip(points, points[1:])
                  if math.dist(a, b) >= 1e-9]
        if not curves:
            raise ExportError(f"{kind} approximation produced no planar geometry.") from exc
        warnings.append(f"Approximated {kind} with {len(curves)} line segments; "
                        f"deflection tolerance {options.tolerance:g} mm. Cause: {exc}")
        return curves
