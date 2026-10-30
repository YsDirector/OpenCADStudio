"""Finite half-space clipping and exact cut-face identification."""
from __future__ import annotations

from dataclasses import replace
import math
from typing import Any

from .model import ExportError, ExportOptions, SectionSpec, ViewSpec, dot
from .sources import SourceShape


def clip_sources(sources: list[SourceShape], spec: SectionSpec) -> tuple[list[SourceShape], list[tuple[int, Any]]]:
    """Return retained solids and newly exposed planar cut faces, per source.

    Each source is clipped independently to preserve contact/material boundaries;
    all retained shapes are later projected together for mutual occlusion.
    """
    import FreeCAD as App
    import Part
    origin = App.Vector(*spec.origin)
    normal = App.Vector(*spec.normal)
    radius = 1.0
    for source in sources:
        if not source.shape.Solids:
            raise ExportError(f"{source.label}: full sections require closed solids, not open faces/wires.")
        # Refuse mixed free faces/wires rather than silently discarding them.
        solids = source.shape.Solids
        if (abs(sum(s.Volume for s in solids) - source.shape.Volume) > 1e-6
                or len(source.shape.Faces) > sum(len(s.Faces) for s in solids)
                or len(source.shape.Edges) > sum(len(s.Edges) for s in solids)):
            raise ExportError(f"{source.label}: mixed solid and open face/wire geometry cannot be sectioned safely.")
        bb = source.shape.BoundBox
        radius = max(radius, *(math.dist((x, y, z), spec.origin)
                               for x in (bb.XMin, bb.XMax)
                               for y in (bb.YMin, bb.YMax)
                               for z in (bb.ZMin, bb.ZMax)))
    radius = radius * 1.1 + 1.0
    up = (0, 0, 1) if abs(spec.normal[2]) < 0.9 else (0, 1, 0)
    right, upwards, n = ViewSpec("section-plane", spec.normal, up).basis()
    matrix = App.Matrix()
    for i in range(3):
        for j, axis in enumerate((right, upwards, n)):
            setattr(matrix, f"A{i + 1}{j + 1}", axis[i])
        setattr(matrix, f"A{i + 1}4", spec.origin[i])
    box_origin = App.Vector(-radius, -radius, -radius if spec.keep == "negative" else 0)
    tool = Part.makeBox(2 * radius, 2 * radius, radius, box_origin)
    tool.transformShape(matrix)
    retained, faces = [], []
    any_cut = False

    def planar_key(face):
        c, axis = face.CenterOfMass, face.Surface.Axis
        return (round(c.x, 7), round(c.y, 7), round(c.z, 7), round(face.Area, 7),
                round(axis.x, 7), round(axis.y, 7), round(axis.z, 7))

    def original_coplanar(solid):
        keys = set()
        for face in solid.Faces:
            if type(face.Surface).__name__ != "Plane":
                continue
            if (abs(face.Surface.Axis.dot(normal)) >= 1.0 - 1e-8
                    and abs((face.CenterOfMass - origin).dot(normal)) < 1e-6):
                keys.add(planar_key(face))
        return keys

    for index, source in enumerate(sources):
        kept = []
        for solid in source.shape.Solids:
            try:
                cut = solid.common(tool)
            except Exception as exc:
                raise ExportError(f"{source.label}: section boolean failed: {exc}") from exc
            if cut.isNull() or not cut.Solids or cut.Volume < 1e-12:
                continue
            if not cut.isValid():
                raise ExportError(f"{source.label}: section produced invalid BRep geometry.")
            kept.append(cut)
            removed = solid.Volume - cut.Volume
            # Classify per SOLID: a fully retained solid in the same compound must
            # not make its coplanar original face look like a new cut surface.
            if removed <= max(1e-8, solid.Volume * 1e-10):
                continue
            any_cut = True
            original = original_coplanar(solid)
            for face in cut.Faces:
                if type(face.Surface).__name__ != "Plane":
                    continue
                axis = face.Surface.Axis
                alignment = abs(axis.dot(normal))
                distance = abs((face.CenterOfMass - origin).dot(normal))
                if (alignment >= 1.0 - 1e-8 and distance < 1e-6 and face.Area > 1e-10
                        and planar_key(face) not in original):
                    faces.append((index, face))
        if kept:
            retained.append(replace(source, shape=kept[0] if len(kept) == 1
                                    else Part.makeCompound(kept)))
    if not retained or not any_cut or not faces:
        raise ExportError("The section plane does not cross the interior of the selected solids.")
    return retained, faces


def _curve_bounds(loops: list[dict]) -> tuple[float, float, float, float]:
    """Projected 2D bounds of all hatch boundary curves, for auto scaling."""
    xs: list[float] = []
    ys: list[float] = []
    for loop in loops:
        for curve in loop["edges"]:
            for key in ("start", "end", "center"):
                point = curve.get(key)
                if point and len(point) >= 2:
                    xs.append(float(point[0]))
                    ys.append(float(point[1]))
            radius = curve.get("radius")
            center = curve.get("center")
            if radius and center:
                xs.extend((float(center[0]) - float(radius), float(center[0]) + float(radius)))
                ys.extend((float(center[1]) - float(radius), float(center[1]) + float(radius)))
            for point in curve.get("control_points", []) or []:
                if len(point) >= 2:
                    xs.append(float(point[0]))
                    ys.append(float(point[1]))
    if not xs:
        raise ExportError("Cannot measure a section hatch region for automatic scaling.")
    return min(xs), max(xs), min(ys), max(ys)


def hatch_faces(faces: list[tuple[int, Any]], view: ViewSpec,
                options: ExportOptions, warnings: list[str]) -> list[dict]:
    from .curves import edge_curves
    spec = options.section
    assert spec is not None
    facing = dot(view.direction, spec.normal) * (1 if spec.keep == "negative" else -1)
    if facing <= 1e-8:
        raise ExportError(f"{view.name}: look from the removed half-space to see the cut surface "
                          "(reverse the view direction or section keep side). Edge-on cuts cannot be hatched.")
    result = []
    for index, face in faces:
        loops = []
        outer = face.OuterWire
        for wire in face.Wires:
            curves = []
            for edge in wire.OrderedEdges:
                curves.extend(edge_curves(edge, "hatch", options, warnings, view.basis()))
            if not curves:
                raise ExportError("An exposed section boundary projected to an empty loop.")
            loops.append({"is_outer": wire.isSame(outer), "edges": curves})
        if sum(bool(loop["is_outer"]) for loop in loops) != 1:
            raise ExportError("Cannot identify the section face's unique outer loop.")
        # 相邻零件靠 0°/90° 交替区分（预制图案自带 45° 斜线，不再额外叠加角度）。
        angle = spec.hatch_angle_deg + (index % 2) * 90.0
        scale = 1.0
        spacing = spec.hatch_spacing
        if spec.hatch_auto_scale:
            x0, x1, y0, y1 = _curve_bounds(loops)
            narrow = min(x1 - x0, y1 - y0)
            if narrow > 0:
                # spacing 即“一个图案周期”；scale = 目标间距 / 基础间距。
                target = narrow / spec.hatch_min_lines
                scale = target / spec.hatch_spacing
                scale = max(spec.hatch_scale_min, min(spec.hatch_scale_max, scale))
                spacing = spec.hatch_spacing * scale
        result.append({"loops": loops, "angle_deg": angle % 360.0, "spacing": spacing,
                       "scale": scale, "source_index": index})
    return result
