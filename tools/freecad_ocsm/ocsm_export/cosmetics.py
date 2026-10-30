"""Guard against silently losing unsupported TechDraw-only annotations."""
from __future__ import annotations

from typing import Any
from .model import ExportError, ExportOptions, ViewSpec


def add_cosmetics(output: dict, source_view: Any, view: ViewSpec,
                  options: ExportOptions, warnings: list[str]) -> None:
    counts = {name: len(getattr(source_view, name, []) or [])
              for name in ("CosmeticEdges", "CenterLines", "GeomFormats", "CosmeticVertexes")}
    if not any(counts.values()):
        return
    description = ", ".join(f"{name}={n}" for name, n in counts.items() if n)
    if options.include_cosmetics:
        raise ExportError("The selected TechDraw view contains cosmetic lines/vertices/centerlines or per-edge "
                          f"format overrides ({description}). Their paper/view coordinate mapping is not "
                          "yet verified. Disable 'include cosmetics' explicitly to export only rebuilt "
                          "BRep engineering geometry, or export the original model objects. Nothing was silently dropped.")
    warnings.append(f"Intentionally excluded TechDraw-only cosmetics/centerlines/format overrides: {description}.")
