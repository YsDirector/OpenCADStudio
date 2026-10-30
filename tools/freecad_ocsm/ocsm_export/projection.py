"""Synchronous exact HLR, with an explicitly controlled drawing coordinate frame."""
from __future__ import annotations

import math
import time
from typing import Any

from .curves import edge_curves
from .model import ExportError, ExportOptions, ViewSpec
from .sources import SourceShape


def _compound(edges: list[Any]) -> Any:
    import Part
    if not edges:
        return Part.Shape()
    compound = Part.makeCompound(edges)
    if len(edges) == 1:
        return compound
    # OCCT Boolean union of edges splits partial overlaps and removes duplicate
    # geometry, unlike identity/hash-based de-duplication of distinct OCC edges.
    try:
        return compound.fuse(compound)
    except Exception as exc:
        raise ExportError(f"Cannot normalize overlapping projected edges: {exc}") from exc


def _merge_collinear(entities: list[dict]) -> list[dict]:
    """Join touching collinear visible/hidden LINE pieces into one line."""
    lines = [e for e in entities if e["type"] == "LINE" and e["role"] in ("visible", "hidden")]
    others = [e for e in entities if not (e["type"] == "LINE" and e["role"] in ("visible", "hidden"))]
    chains: list[dict] = []
    endpoints: dict[tuple, list[int]] = {}
    tolerance = 1e-7

    def point_key(point):
        return (round(point[0], 7), round(point[1], 7))

    def register(chain_index):
        chain = chains[chain_index]
        for point in (chain["start"], chain["end"]):
            endpoints.setdefault(point_key(point), []).append(chain_index)

    for entity in lines:
        start, end = entity["start"], entity["end"]
        dx, dy = end[0] - start[0], end[1] - start[1]
        length = math.hypot(dx, dy)
        if length < 1e-9:
            continue
        ux, uy = dx / length, dy / length
        if ux < 0 or (abs(ux) <= 1e-12 and uy < 0):
            ux, uy = -ux, -uy
        offset = ux * start[1] - uy * start[0]
        key = (entity["role"], round(ux, 9), round(uy, 9), round(offset, 7))
        candidates = set(endpoints.get(point_key(start), ())) | set(endpoints.get(point_key(end), ()))
        target = next((i for i in candidates if chains[i]["key"] == key and chains[i]["start"] != chains[i]["end"]), None)
        if target is None:
            chains.append({"key": key, "role": entity["role"], "start": start, "end": end})
            register(len(chains) - 1)
            continue
        chain = chains[target]
        for point in (start, end):
            if math.dist(point, chain["end"]) <= tolerance:
                chain["end"] = end if math.dist(point, start) <= tolerance else start
                break
            if math.dist(point, chain["start"]) <= tolerance:
                chain["start"] = end if math.dist(point, start) <= tolerance else start
                break
        endpoints.clear()
        for index, item in enumerate(chains):
            register(index)
    result = []
    for chain in chains:
        if math.dist(chain["start"], chain["end"]) < 1e-9:
            continue
        result.append({"type": "LINE", "role": chain["role"], "start": chain["start"],
                       "end": chain["end"], "preview_points": [chain["start"], chain["end"]]})
    return result + others


def project_sources(sources: list[SourceShape], view: ViewSpec,
                    options: ExportOptions, warnings: list[str]) -> dict:
    import FreeCAD as App
    import Part
    import TechDraw
    start = time.monotonic()
    shape = Part.makeCompound([source.shape for source in sources])
    matrix = App.Matrix()
    for i, axis in enumerate(view.basis()):
        for j in range(3):
            setattr(matrix, f"A{i + 1}{j + 1}", axis[j])
    shape.transformShape(matrix)
    try:
        groups = TechDraw.projectEx(shape, App.Vector(0, 0, 1))
    except Exception as exc:
        raise ExportError(f"{view.name}: exact hidden-line projection failed: {exc}") from exc
    if len(groups) != 10:
        raise ExportError(f"Unexpected TechDraw.projectEx contract ({len(groups)} groups, expected 10).")
    # V,V1,VN,VO,VI,H,H1,HN,HO,HI. VI/HI are surface isoparameters, NOT ISO views.
    visible_indices = [0, 3]
    hidden_indices = [5, 8] if options.hidden_hard else []
    if options.include_smooth:
        visible_indices.append(1)
    if options.include_seams:
        visible_indices.append(2)
    if (options.hidden_smooth if options.hidden_smooth is not None else options.include_smooth):
        hidden_indices.append(6)
    if (options.hidden_seams if options.hidden_seams is not None else options.include_seams):
        hidden_indices.append(7)
    visible = _compound([edge for i in visible_indices for edge in groups[i].Edges])
    hidden = Part.Shape()
    show_hidden = options.mode == "hidden" or (options.mode == "section" and options.section_hidden)
    if show_hidden:
        hidden = _compound([edge for i in hidden_indices for edge in groups[i].Edges])
        if not hidden.isNull() and not visible.isNull():
            try:
                # Back edges often coincide with front edges after projection.
                # Visible geometry wins; retain only non-overlapping hidden parts.
                hidden = hidden.cut(visible)
            except Exception as exc:
                raise ExportError(f"Cannot resolve visible/hidden edge overlaps: {exc}") from exc
    entities = []
    for role, projected in (("visible", visible), ("hidden", hidden)):
        if not projected.isNull():
            for edge in projected.Edges:
                entities.extend(edge_curves(edge, role, options, warnings))
    entities = _merge_collinear(entities)
    if not entities:
        raise ExportError(f"{view.name}: projection produced no drawable engineering edges.")
    return {"name": view.name, "entities": entities, "hatches": [],
            "metadata": {"direction": list(view.direction), "up": list(view.up),
                         "rotation_deg": view.rotation_deg, "basis": [list(v) for v in view.basis()],
                         "source_objects": [{"name": s.name, "label": s.label} for s in sources],
                         "hlr_group_edge_counts": [len(group.Edges) for group in groups],
                         "projection_seconds": time.monotonic() - start}}
