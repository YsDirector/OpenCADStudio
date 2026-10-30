"""Read selected shapes without recomputing or modifying their documents."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .model import ExportError


@dataclass
class SourceShape:
    name: str
    label: str
    shape: Any
    obj: Any = None


def is_techdraw(obj: Any) -> bool:
    return str(getattr(obj, "TypeId", "")).startswith("TechDraw::")


def view_sources(view: Any) -> list[Any]:
    supported = {"TechDraw::DrawViewPart", "TechDraw::DrawProjGroupItem", "TechDraw::DrawViewSection"}
    if getattr(view, "TypeId", "") not in supported:
        raise ExportError("Only a single ordinary TechDraw view or planar full section is supported; "
                          "complex/detail/projection-group objects are not silently flattened.")
    result = list(getattr(view, "Source", []) or [])
    for item in getattr(view, "XSource", []) or []:
        if isinstance(item, (tuple, list)):
            subnames = tuple(item[1] or ()) if len(item) > 1 else ()
            if subnames:
                raise ExportError("The selected TechDraw view limits XSource to sub-elements "
                                  f"({', '.join(map(str, subnames))}); projecting only those sub-elements is "
                                  "not supported. Select the whole source object instead. Nothing was widened silently.")
            result.append(item[0] if item else item)
        else:
            result.append(item)
    if not result:
        raise ExportError("The selected TechDraw view has no source objects.")
    return result


def _parent_transform(obj: Any) -> Any:
    import FreeCAD as App
    transform = App.Placement()
    seen = set()
    while callable(getattr(obj, "getParentGeoFeatureGroup", None)):
        obj = obj.getParentGeoFeatureGroup()
        if obj is None:
            break
        key = (getattr(getattr(obj, "Document", None), "Name", ""), obj.Name)
        if key in seen:
            raise ExportError("Cyclic placement/container hierarchy.")
        seen.add(key)
        transform = obj.Placement.multiply(transform)
    return transform


def collect_sources(objects: Any) -> tuple[list[SourceShape], Any | None]:
    """Resolve Body tips/containers/Links once, applying ancestor placements once.

    A directly selected object is included even if hidden. Container children are
    filtered by their own visibility. A Body is a single final Shape, never its
    historical features. Raw TopoShapes are accepted for script/test callers.
    """
    import Part
    if objects is None:
        raise ExportError("Select at least one model object or a TechDraw view.")
    objects = list(objects)
    if not objects:
        raise ExportError("Select at least one model object or a TechDraw view.")
    selected_view = None
    if any(is_techdraw(obj) for obj in objects):
        if len(objects) != 1:
            raise ExportError("Select one TechDraw view alone, or select model objects instead.")
        selected_view = objects[0]
        objects = view_sources(selected_view)
    result: list[SourceShape] = []
    seen: set[tuple[str, str]] = set()
    visiting: set[int] = set()

    def visit(obj: Any, explicit: bool = True) -> None:
        if obj is None:
            raise ExportError("The source contains a missing object reference.")
        if isinstance(obj, Part.Shape):
            shape = obj.copy()
            name = f"Shape{len(result) + 1}"
            label = name
            original = None
        else:
            if not explicit:
                vo = getattr(obj, "ViewObject", None)
                if vo is not None and not vo.Visibility:
                    return
            key = (getattr(getattr(obj, "Document", None), "Name", ""), obj.Name)
            if key in seen:
                return
            seen.add(key)
            type_id = str(getattr(obj, "TypeId", ""))
            # Do not recurse into a Body (Tip only) or a Link (instance Shape).
            container = type_id in ("App::Part", "App::DocumentObjectGroup",
                                    "App::DocumentObjectGroupPython")
            if container:
                if id(obj) in visiting:
                    raise ExportError("Cyclic object container.")
                visiting.add(id(obj))
                for child in obj.Group:
                    visit(child, explicit=False)
                visiting.remove(id(obj))
                return
            name, label, original = obj.Name, obj.Label, obj
            if not hasattr(obj, "Shape"):
                if explicit:
                    raise ExportError(f"{label}: object has no BRep Shape (meshes are not silently tessellated).")
                return
            states = list(getattr(obj, "State", []))
            if any(str(s) in ("Invalid", "Error") for s in states):
                raise ExportError(f"{label}: source has an error; repair/recompute it before export.")
            if "Touched" in states:
                raise ExportError(f"{label}: source is not recomputed; recompute the model before export.")
            shape = Part.getShape(obj).copy()
            if shape.isNull():
                raise ExportError(f"{label}: empty BRep Shape.")
            # getShape includes obj's own placement (also LinkPlacement) but not
            # the selected object's App::Part/Body ancestors.
            shape.transformShape(_parent_transform(obj).toMatrix())
        if shape.isNull() or not shape.Edges:
            raise ExportError(f"{label}: empty BRep Shape.")
        if not shape.isValid():
            raise ExportError(f"{label}: invalid BRep Shape.")
        result.append(SourceShape(name, label, shape, original))

    for obj in objects:
        visit(obj)
    if not result:
        raise ExportError("The selection contains no drawable visible BRep objects.")
    # A Body Tip and one of its own features (or two coincident equal parts)
    # resolve to identical geometry; export it once instead of duplicating HLR
    # edges and section hatches.
    unique: list[SourceShape] = []
    keys: set[tuple] = set()
    for source in result:
        key = _shape_key(source.shape)
        if key not in keys:
            keys.add(key)
            unique.append(source)
    return unique, selected_view


def _shape_key(shape: Any) -> tuple:
    """Stable geometry fingerprint; TopoDS isSame/isEqual compare TShape identity."""
    bb = shape.BoundBox
    faces = tuple(sorted((round(f.CenterOfMass.x, 6), round(f.CenterOfMass.y, 6),
                          round(f.CenterOfMass.z, 6), round(f.Area, 6))
                         for f in shape.Faces))
    return (len(shape.Solids), len(shape.Faces), len(shape.Edges),
            round(shape.Volume, 9), round(shape.Area, 9),
            round(bb.XMin, 9), round(bb.YMin, 9), round(bb.ZMin, 9),
            round(bb.XMax, 9), round(bb.YMax, 9), round(bb.ZMax, 9), faces)
