"""Dependency-free options and the versioned interchange contract.

Coordinates are millimetres in a right-handed XY drawing plane.  The view
``direction`` points from the model towards the viewer (FreeCAD convention).
The scene is JSON-compatible: schema_version=1, views=[{name, entities,
hatches, metadata}], metadata={...}, warnings=[...].  Curves use LINE, CIRCLE,
ARC, ELLIPSE or SPLINE; roles are visible, hidden, thin or center.  A hatch
contains loops=[{is_outer: bool, edges: [curve, ...]}], angle_deg and spacing.
ARC angles are degrees; ELLIPSE parameters are radians.  Curve ``ccw`` is
significant for directed hatch edges.  SPLINEs use degree, control_points,
knots (expanded knot vector), optional weights, and periodic/closed flags.
A view's effective display mode is its own ``mode`` when set, otherwise the
export default; multi-view placement uses metadata["layout"] =
first_angle|third_angle.
"""
from __future__ import annotations

from dataclasses import dataclass, field
import math
from typing import Any

SCHEMA_VERSION = 1
LAYERS = {
    "visible": ("1轮廓实线层", 7, "Continuous", 35),
    "thin": ("2细线层", 4, "Continuous", 18),
    "center": ("3中心线层", 1, "CENTER2", 18),
    "hidden": ("4虚线层", 6, "DASHED2", 18),
    "hatch": ("5剖面线层", 2, "Continuous", 18),
}
Vec3 = tuple[float, float, float]


def vector(value: Any, label: str = "vector") -> Vec3:
    try:
        result = tuple(float(v) for v in value)
    except (TypeError, ValueError) as exc:
        raise ValueError(f"{label} must contain three finite numbers") from exc
    if len(result) != 3 or not all(math.isfinite(v) for v in result):
        raise ValueError(f"{label} must contain three finite numbers")
    return result  # type: ignore[return-value]


def dot(a: Vec3, b: Vec3) -> float:
    return sum(x * y for x, y in zip(a, b))


def cross(a: Vec3, b: Vec3) -> Vec3:
    return (a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def unit(value: Any, label: str = "direction") -> Vec3:
    value = vector(value, label)
    length = math.sqrt(dot(value, value))
    if length < 1e-12:
        raise ValueError(f"{label} must be nonzero")
    return tuple(v / length for v in value)  # type: ignore[return-value]


@dataclass(frozen=True)
class ViewSpec:
    name: str = "front"
    direction: Vec3 = (0.0, -1.0, 0.0)
    up: Vec3 = (0.0, 0.0, 1.0)
    rotation_deg: float = 0.0
    mode: str | None = None            # None = inherit ExportOptions.mode
    section_hidden: bool | None = None  # None = inherit ExportOptions.section_hidden

    def __post_init__(self) -> None:
        if not self.name.strip():
            raise ValueError("view name must not be empty")
        object.__setattr__(self, "direction", unit(self.direction))
        object.__setattr__(self, "up", unit(self.up, "up"))
        if abs(dot(self.direction, self.up)) > 1.0 - 1e-10:
            raise ValueError("up must not be parallel to direction")
        if not math.isfinite(self.rotation_deg):
            raise ValueError("rotation must be finite")
        if self.mode not in (None, "visible", "hidden", "section"):
            raise ValueError("view mode must be visible, hidden, section or None")
        if self.section_hidden is not None and type(self.section_hidden) is not bool:
            raise ValueError("view section_hidden must be a bool or None")

    def basis(self) -> tuple[Vec3, Vec3, Vec3]:
        """Return screen right, screen up, viewer normal, including rotation."""
        normal = self.direction
        right = unit(cross(self.up, normal))
        up = cross(normal, right)
        angle = math.radians(self.rotation_deg)
        c, s = math.cos(angle), math.sin(angle)
        return (tuple(c * a - s * b for a, b in zip(right, up)),
                tuple(s * a + c * b for a, b in zip(right, up)), normal)


STANDARD_VIEWS = {
    "front": ViewSpec(),
    "back": ViewSpec("back", (0, 1, 0)),
    "left": ViewSpec("left", (-1, 0, 0)),
    "right": ViewSpec("right", (1, 0, 0)),
    "top": ViewSpec("top", (0, 0, 1), (0, 1, 0)),
    "bottom": ViewSpec("bottom", (0, 0, -1), (0, -1, 0)),
    "iso": ViewSpec("iso", (1, -1, 1)),
}


@dataclass(frozen=True)
class SectionSpec:
    origin: Vec3 = (0.0, 0.0, 0.0)
    normal: Vec3 = (0.0, -1.0, 0.0)
    keep: str = "negative"
    hatch_angle_deg: float = 0.0
    hatch_spacing: float = 3.175
    hatch_enabled: bool = True
    # Auto scale: 间距 = 该剖面分区最窄边 / hatch_min_lines，限幅到
    # [hatch_spacing*hatch_scale_min, hatch_spacing*hatch_scale_max]。
    # 大件用大比例、小件用小比例，避免“大件稀、小件空”。
    hatch_auto_scale: bool = True
    hatch_min_lines: float = 6.0
    hatch_scale_min: float = 0.4
    hatch_scale_max: float = 2.0

    def __post_init__(self) -> None:
        object.__setattr__(self, "origin", vector(self.origin, "section origin"))
        object.__setattr__(self, "normal", unit(self.normal, "section normal"))
        if self.keep not in ("negative", "positive"):
            raise ValueError("section keep must be negative or positive")
        if not math.isfinite(self.hatch_angle_deg):
            raise ValueError("hatch angle must be finite")
        # 预制图案（ANSI31）自带 45° 斜线；HATCH 角度只是“整体转 90° 区分相邻零件”。
        # 旧调用传 45/135 的本意是“45°/135° 斜线”，因此吸附为 0/90（不报错），
        # 避免把角度烘进制图定义后再转一次导致异常角度。
        snapped = 0.0 if (self.hatch_angle_deg % 180.0) < 45.0 else 90.0
        if snapped != self.hatch_angle_deg:
            object.__setattr__(self, "hatch_angle_deg", snapped)
        if not math.isfinite(self.hatch_spacing) or self.hatch_spacing <= 0:
            raise ValueError("hatch spacing must be positive and finite")
        if type(self.hatch_enabled) is not bool:
            raise ValueError("hatch_enabled must be a bool")
        if type(self.hatch_auto_scale) is not bool:
            raise ValueError("hatch_auto_scale must be a bool")
        if not math.isfinite(self.hatch_min_lines) or self.hatch_min_lines < 2:
            raise ValueError("hatch_min_lines must be >= 2")
        if not (0 < self.hatch_scale_min <= 1 <= self.hatch_scale_max):
            raise ValueError("hatch scale limits must satisfy 0 < min <= 1 <= max")


@dataclass(frozen=True)
class ExportOptions:
    views: tuple[ViewSpec, ...] = field(default_factory=lambda: (STANDARD_VIEWS["front"],))
    mode: str = "visible"
    layout: str = "first_angle"
    include_smooth: bool = False
    include_seams: bool = False
    section: SectionSpec | None = None
    section_hidden: bool = False
    tolerance: float = 0.01
    allow_approximation: bool = False
    include_cosmetics: bool = True
    # None means the same setting as visible smooth/seam edges. Existing
    # TechDraw views may have independent visible/hidden category switches.
    hidden_hard: bool = True
    hidden_smooth: bool | None = None
    hidden_seams: bool | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "views", tuple(self.views))
        if not self.views or any(not isinstance(v, ViewSpec) for v in self.views):
            raise ValueError("at least one valid ViewSpec is required")
        if len({v.name for v in self.views}) != len(self.views):
            raise ValueError("view names must be unique")
        if self.mode not in ("visible", "hidden", "section"):
            raise ValueError("mode must be visible, hidden or section")
        if self.layout not in ("first_angle", "third_angle"):
            raise ValueError("layout must be first_angle or third_angle")
        if self.mode == "section" and not isinstance(self.section, SectionSpec):
            raise ValueError("section mode requires a SectionSpec")
        if any(v.mode == "section" for v in self.views) and not isinstance(self.section, SectionSpec):
            raise ValueError("a per-view section mode requires a SectionSpec")
        if not math.isfinite(self.tolerance) or self.tolerance <= 0:
            raise ValueError("tolerance must be positive and finite")

    def mode_of(self, view: ViewSpec) -> str:
        return view.mode or self.mode

    def section_hidden_for(self, view: ViewSpec) -> bool:
        return self.section_hidden if view.section_hidden is None else view.section_hidden


class ExportError(RuntimeError):
    """Actionable geometry/export failure; never report a partial file as success."""
