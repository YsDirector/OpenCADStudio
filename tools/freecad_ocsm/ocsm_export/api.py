"""Public, non-destructive FreeCAD export API. FreeCAD imports are lazy."""
from __future__ import annotations

from dataclasses import asdict, replace
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from typing import Any

from . import __version__
from .model import (SCHEMA_VERSION, ExportError, ExportOptions, SectionSpec,
                    ViewSpec, cross, dot, unit)


def options_from_techdraw(view: Any) -> ExportOptions:
    """Read a supported TechDraw view's model-space settings (never paper scale)."""
    from .sources import view_sources
    view_sources(view)  # validates type and source references
    if getattr(view, "Perspective", False):
        raise ExportError("Perspective TechDraw views are not supported; use an orthographic view.")
    direction = unit(tuple(view.Direction))
    # Prefer FreeCAD's resolved right vector (ProjGroupItem RotationVector,
    # section BaseView, legacy zero XDirection). Never silently pick an
    # unrelated up direction: a wrong roll is a wrong engineering view.
    raw = None
    getter = getattr(view, "getXDirection", None)
    if callable(getter):
        try:
            raw = tuple(getter())
        except Exception:
            raw = None
    if raw is None or sum(v * v for v in raw) < 1e-18:
        raw = tuple(getattr(view, "XDirection", (0, 0, 0)) or (0, 0, 0))
    if (sum(v * v for v in raw) < 1e-18
            and getattr(view, "TypeId", "") == "TechDraw::DrawProjGroupItem"):
        raw = tuple(getattr(view, "RotationVector", (0, 0, 0)) or (0, 0, 0))
    if sum(v * v for v in raw) < 1e-18:
        raise ExportError("The TechDraw view has no usable right/X direction; set XDirection "
                          "or export the model objects instead of this view.")
    right = unit(raw)
    projected = tuple(right[i] - dot(right, direction) * direction[i] for i in range(3))
    length = math.sqrt(dot(projected, projected))
    if length < 1e-7:
        raise ExportError("The TechDraw view's X direction is parallel to its view direction; "
                          "the drawing plane is undefined.")
    right = tuple(v / length for v in projected)
    up = cross(direction, right)
    spec = ViewSpec(view.Name, direction, up, float(view.Rotation))
    section = None
    hard_hidden = bool(getattr(view, "HardHidden", False))
    smooth_hidden = bool(getattr(view, "SmoothHidden", False))
    seam_hidden = bool(getattr(view, "SeamHidden", False))
    any_hidden = hard_hidden or smooth_hidden or seam_hidden
    mode = "hidden" if any_hidden else "visible"
    if getattr(view, "TypeId", "") == "TechDraw::DrawViewSection":
        if bool(getattr(view, "UsePreviousCut", False)):
            raise ExportError("Sections based on a previous cut are not supported in this version.")
        if bool(getattr(view, "FuseBeforeCut", False)):
            raise ExportError("Disable FuseBeforeCut before exporting; preserving part boundaries requires separate solids.")
        if bool(getattr(view, "TrimAfterCut", False)):
            raise ExportError("TrimAfterCut sections are not supported; disable TrimAfterCut or export "
                              "the model objects with an explicit section plane.")
        display = str(getattr(view, "CutSurfaceDisplay", "") or "")
        if display == "Solid":
            raise ExportError("Solid-filled sections are not supported; choose a pattern hatch "
                              "(PatHatch/SvgHatch) or NoCut.")
        scale = float(getattr(view, "HatchScale", 1.0) or 1.0)
        rotation = float(getattr(view, "HatchRotation", 0.0) or 0.0)
        if not math.isfinite(scale) or scale <= 0:
            raise ExportError("The TechDraw HatchScale must be positive and finite.")
        if not math.isfinite(rotation):
            raise ExportError("The TechDraw HatchRotation must be finite.")
        section = SectionSpec(tuple(view.SectionOrigin), tuple(view.SectionNormal),
                              hatch_angle_deg=(45.0 + rotation) % 360.0,
                              hatch_spacing=3.175 * scale,
                              hatch_enabled=display != "NoCut")
        mode = "section"
    spec = replace(spec, mode=mode, section_hidden=any_hidden)
    return ExportOptions(views=(spec,), mode=mode,
                         include_smooth=bool(getattr(view, "SmoothVisible", False)),
                         include_seams=bool(getattr(view, "SeamVisible", False)),
                         section=section, section_hidden=any_hidden,
                         hidden_hard=hard_hidden, hidden_smooth=smooth_hidden,
                         hidden_seams=seam_hidden)


def build_scene(objects: Any, options: ExportOptions) -> dict:
    """Build versioned planar geometry using selected BReps, without document edits."""
    import FreeCAD as App
    import Part
    from .sources import collect_sources
    from .projection import project_sources
    from .section import clip_sources, hatch_faces
    sources, selected_view = collect_sources(objects)
    warnings: list[str] = []
    metadata = {"generator": "freecad-ocsm-export", "generator_version": __version__,
                "freecad_version": list(App.Version()), "occt_version": getattr(Part, "OCC_VERSION", None),
                "units": "mm", "model_scale": 1.0, "layout": options.layout,
                "settings": asdict(options),
                "sources": [{"name": s.name, "label": s.label,
                             "document": getattr(getattr(s.obj, "Document", None), "Name", None),
                             "solids": len(s.shape.Solids), "faces": len(s.shape.Faces),
                             "edges": len(s.shape.Edges)} for s in sources]}
    if selected_view is not None:
        if getattr(selected_view, "Perspective", False):
            raise ExportError("A perspective TechDraw view cannot be exported as an engineering orthographic view.")
        metadata["selected_techdraw_view"] = selected_view.Name
        metadata["ignored_paper_scale"] = float(selected_view.Scale)
    cut_faces = []
    base_sources = sources
    section_sources = sources
    section_modes = [options.mode_of(spec) == "section" for spec in options.views]
    if any(section_modes):
        section_sources, cut_faces = clip_sources(sources, options.section)
        if selected_view is not None:
            offset = tuple(getattr(selected_view, "HatchOffset", (0, 0, 0)) or (0, 0, 0))
            if any(abs(float(v)) > 1e-12 for v in offset):
                warnings.append("TechDraw HatchOffset is not mapped; the exported pattern starts at the origin.")
            pattern = str(getattr(selected_view, "NameGeomPattern", "") or "")
            if pattern and pattern.upper() != "OCSM_SECTION":
                warnings.append(f"TechDraw hatch pattern '{pattern}' is exported as OCSM_SECTION; "
                                "pattern geometry differs from the original view.")
    views = []
    for spec, is_section in zip(options.views, section_modes):
        view_options = replace(options, mode="section" if is_section else options.mode_of(spec),
                               section_hidden=options.section_hidden_for(spec))
        view = project_sources(section_sources if is_section else base_sources,
                               spec, view_options, warnings)
        if is_section and cut_faces and options.section is not None and options.section.hatch_enabled:
            view["hatches"] = hatch_faces(cut_faces, spec, view_options, warnings)
        if selected_view is not None:
            from .cosmetics import add_cosmetics
            add_cosmetics(view, selected_view, spec, options, warnings)
        views.append(view)
    return {"schema_version": SCHEMA_VERSION, "views": views, "metadata": metadata,
            "warnings": list(dict.fromkeys(warnings))}


def _safe_target(path: Any) -> Path:
    """Reject symlinks/non-files without resolving the final path component.

    ``Path.resolve()`` would silently replace a symlink's target, bypassing the
    direct writer's refusal. A symlinked parent directory is still allowed.
    """
    if isinstance(path, bool) or not isinstance(path, (str, os.PathLike)):
        raise ValueError("target path must be a filesystem path")
    text = str(os.fspath(path)).strip()
    if not text:
        raise ValueError("target path must not be empty")
    target = Path(os.path.abspath(os.path.expanduser(text)))
    if target.is_symlink():
        raise ExportError(f"Refusing to write through a symbolic link: {target}")
    if target.exists() and not target.is_file():
        raise ExportError(f"Target is not a regular file: {target}")
    return target


def _external_writer(scene: dict, path: Path, overwrite: bool, executable: str | None) -> dict:
    package_root = Path(__file__).resolve().parent.parent
    configured = executable or os.environ.get("OCSM_EXPORT_PYTHON")
    if not configured:
        candidate = package_root / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        if candidate.is_file():
            configured = str(candidate)
    if not configured:
        raise ExportError("ezdxf is not available in FreeCAD's Python. Install the dependency for that "
                          "Python, create tools/freecad_ocsm/.venv, or set OCSM_EXPORT_PYTHON to an "
                          "interpreter containing ezdxf. No dependencies are installed automatically.")
    configured = os.path.expanduser(configured)
    executable_path = shutil.which(configured) or configured
    env = os.environ.copy()
    env["PYTHONPATH"] = str(package_root) + os.pathsep + env.get("PYTHONPATH", "")
    with tempfile.TemporaryDirectory(prefix="ocsm-dxf-") as temp:
        scene_path = Path(temp) / "scene.json"
        scene_path.write_text(json.dumps(scene, ensure_ascii=False, allow_nan=False), encoding="utf-8")
        command = [executable_path, "-m", "ocsm_export.writer_cli", str(scene_path), str(path)]
        if overwrite:
            command.append("--overwrite")
        try:
            process = subprocess.run(command, capture_output=True, text=True, encoding="utf-8",
                                     env=env, timeout=120, check=False)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise ExportError(f"Cannot run the configured DXF writer: {exc}") from exc
    if process.returncode:
        raise ExportError(f"DXF writer failed: {process.stderr.strip() or process.stdout.strip()}")
    try:
        return json.loads(process.stdout)
    except (json.JSONDecodeError, ValueError) as exc:
        raise ExportError("The external DXF writer returned an invalid report.") from exc


def export_scene(scene: dict, path: Any, *, overwrite: bool = False,
                 python_executable: str | None = None) -> dict:
    # Validate both knobs before any subprocess/import so a stray truthy value
    # cannot overwrite a file that the direct writer would have refused.
    if type(overwrite) is not bool:
        raise ValueError("overwrite must be a bool")
    target = _safe_target(path)
    if python_executable is not None:
        return _external_writer(scene, target, overwrite, python_executable)
    try:
        from .dxf_writer import write_dxf
    except ImportError as exc:
        if exc.name not in ("ezdxf", "numpy", "numpy._core"):
            raise
        return _external_writer(scene, target, overwrite, None)
    return write_dxf(scene, target, overwrite=overwrite)


def export_objects(objects: Any, path: Any, options: ExportOptions | None = None, *,
                   overwrite: bool = False, python_executable: str | None = None) -> dict:
    objects = list(objects or [])
    if options is None:
        from .sources import is_techdraw
        options = options_from_techdraw(objects[0]) if len(objects) == 1 and is_techdraw(objects[0]) else ExportOptions()
    scene = build_scene(objects, options)
    return export_scene(scene, path, overwrite=overwrite, python_executable=python_executable)
