"""FreeCAD selection exporter. Importing this module does not import Qt/FreeCAD.

The modeless dialog owns no document objects and never uses the Control panel.
All geometry is rebuilt on Preview/Export; the displayed SVG is only a snapshot.
"""
from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import replace
import math
from pathlib import Path
import re
import traceback
from typing import Any

from .model import ExportError, ExportOptions, SectionSpec, STANDARD_VIEWS, ViewSpec
from .preview import scene_summary, scene_svg

_dialog = None
VIEW_LABELS = {
    "front": "前", "back": "后", "left": "左", "right": "右",
    "top": "上", "bottom": "下", "iso": "ISO（正交）",
    "camera": "当前相机（正交视向）", "custom": "自定义",
}


def _number(value: Any, label: str) -> float:
    if isinstance(value, bool):
        raise ValueError(f"{label} 必须是有限数值")
    try:
        value = float(value)
    except (ValueError, TypeError, OverflowError) as exc:
        raise ValueError(f"{label} 必须是有限数值") from exc
    if not math.isfinite(value):
        raise ValueError(f"{label} 必须是有限数值")
    return value


def _vector(value: Any, label: str) -> tuple[float, float, float]:
    if isinstance(value, str):
        value = re.split(r"[,，\s]+", value.strip())
    try:
        items = tuple(value)
    except TypeError as exc:
        raise ValueError(f"{label} 需要三个有限数值，例如 0, -1, 0") from exc
    if len(items) != 3:
        raise ValueError(f"{label} 需要三个有限数值，例如 0, -1, 0")
    return tuple(_number(v, label) for v in items)  # type: ignore[return-value]


def _flag(values: Mapping, key: str, default: bool = False) -> bool:
    value = values.get(key, default)
    if not isinstance(value, bool):
        raise ValueError(f"{key} 必须是布尔值")
    return value


def _optional_flag(values: Mapping, key: str) -> bool | None:
    value = values.get(key)
    if value is not None and not isinstance(value, bool):
        raise ValueError(f"{key} 必须是布尔值或 None（跟随可见设置）")
    return value


def selection_center(objects: Any) -> tuple[float, float, float]:
    """Read the union BRep bounds in world coordinates; never edits a document."""
    from .sources import collect_sources
    sources, _ = collect_sources(objects)
    boxes = [source.shape.BoundBox for source in sources]
    return tuple((min(getattr(b, axis + 'Min') for b in boxes)
                  + max(getattr(b, axis + 'Max') for b in boxes)) / 2
                 for axis in ('X', 'Y', 'Z'))


def camera_view(direction: Any, up: Any) -> ViewSpec:
    """FreeCAD getViewDirection points INTO the screen; our normal points OUT."""
    direction = _vector(direction, "当前相机方向")
    return ViewSpec("camera", tuple(-v for v in direction), _vector(up, "当前相机上方向"))


def options_from_form(values: Mapping, *, camera_direction: Any = None,
                      camera_up: Any = None) -> ExportOptions:
    """Pure form-to-options conversion, also usable without FreeCAD or Qt."""
    if not isinstance(values, Mapping):
        raise ValueError("导出参数必须是一个映射")
    keys = values.get("views", ["front"])
    if not isinstance(keys, Sequence) or isinstance(keys, (str, bytes)):
        raise ValueError("views 必须是视向名称列表")
    specs = []
    view_modes = values.get("view_modes") or {}
    if not isinstance(view_modes, Mapping):
        raise ValueError("view_modes 必须是视向到模式的映射")
    mode_tokens = {"inherit": (None, None), "visible": ("visible", None),
                   "hidden": ("hidden", None), "section": ("section", False),
                   "section_hidden": ("section", True)}

    def display(key):
        token = view_modes.get(key, "inherit")
        if token not in mode_tokens:
            raise ValueError(f"不支持的视图输出模式: {token!r}")
        return mode_tokens[token]

    for key in keys:
        if not isinstance(key, str) or key not in VIEW_LABELS:
            raise ValueError(f"不支持的视向: {key!r}")
        view_mode, section_hidden = display(key)
        if key in STANDARD_VIEWS:
            spec = replace(STANDARD_VIEWS[key], mode=view_mode, section_hidden=section_hidden)
        elif key == "camera":
            if camera_direction is None or camera_up is None:
                raise ValueError("当前相机不可用：请激活一个三维视图或改选其他视向")
            spec = replace(camera_view(camera_direction, camera_up),
                           mode=view_mode, section_hidden=section_hidden)
        else:
            name = values.get("custom_name", "custom")
            if not isinstance(name, str):
                raise ValueError("自定义视向名称必须是文本")
            spec = ViewSpec(name,
                            _vector(values.get("direction", (0, -1, 0)), "视向"),
                            _vector(values.get("up", (0, 0, 1)), "上方向"),
                            _number(values.get("rotation_deg", 0), "视图旋转"),
                            view_mode, section_hidden)
        specs.append(spec)
    mode = values.get("mode", "visible")
    if not isinstance(mode, str):
        raise ValueError("mode 必须是 visible / hidden / section")
    any_section = mode == "section" or any(spec.mode == "section" for spec in specs)
    section = None
    if any_section:
        section = SectionSpec(
            _vector(values.get("section_origin", (0, 0, 0)), "剖切原点"),
            _vector(values.get("section_normal", (0, -1, 0)), "剖切法向"),
            values.get("section_keep", "negative"),
            _number(values.get("hatch_angle_deg", 0), "剖面线角度"),
            _number(values.get("hatch_spacing", 3.175), "剖面线间距"),
            _flag(values, "hatch_enabled", True))
    return ExportOptions(
        views=tuple(specs), mode=mode, layout=values.get("layout", "first_angle"),
        include_smooth=_flag(values, "include_smooth"),
        include_seams=_flag(values, "include_seams"), section=section,
        section_hidden=_flag(values, "section_hidden"),
        tolerance=_number(values.get("tolerance", 0.01), "误差容限"),
        allow_approximation=_flag(values, "allow_approximation"),
        include_cosmetics=_flag(values, "include_cosmetics", True),
        hidden_hard=_flag(values, "hidden_hard", True),
        hidden_smooth=_optional_flag(values, "hidden_smooth"),
        hidden_seams=_optional_flag(values, "hidden_seams"))


def form_from_options(options: ExportOptions) -> dict:
    """Losslessly represent a TechDraw view (or preset views + one custom view).

    Unsupported combinations are rejected before any UI fields are changed.
    """
    if not isinstance(options, ExportOptions):
        raise ValueError("需要 ExportOptions")
    keys, custom, modes = [], None, {}
    for spec in options.views:
        bare = replace(spec, mode=None, section_hidden=None)
        key = next((key for key, preset in STANDARD_VIEWS.items() if bare == preset), None)
        if key is not None:
            keys.append(key)
        elif custom is None:
            custom = spec
            keys.append("custom")
        else:
            raise ValueError("此界面仅支持一个自定义视向；多个任意视向请使用 Python API")
        if spec.mode is None:
            token = "inherit"
        elif spec.mode == "section":
            token = "section_hidden" if spec.section_hidden else "section"
        else:
            token = spec.mode
        modes[keys[-1]] = token
    section = options.section or SectionSpec()
    custom = custom or ViewSpec("custom")
    return dict(views=keys, mode=options.mode, layout=options.layout, view_modes=modes,
                custom_name=custom.name,
                direction=custom.direction, up=custom.up, rotation_deg=custom.rotation_deg,
                include_smooth=options.include_smooth, include_seams=options.include_seams,
                section_origin=section.origin, section_normal=section.normal, section_keep=section.keep,
                hatch_angle_deg=section.hatch_angle_deg, hatch_spacing=section.hatch_spacing,
                hatch_enabled=section.hatch_enabled,
                section_hidden=options.section_hidden, tolerance=options.tolerance,
                allow_approximation=options.allow_approximation, include_cosmetics=options.include_cosmetics,
                hidden_hard=options.hidden_hard, hidden_smooth=options.hidden_smooth,
                hidden_seams=options.hidden_seams)


def target_path(value: str) -> Path:
    """Validate a user-specified target without creating directories or files."""
    if not isinstance(value, str) or not value.strip():
        raise ValueError("请选择目标 DXF 路径")
    path = Path(value.strip()).expanduser()
    if path.is_dir():
        raise ValueError("目标路径指向目录，请指定 DXF 文件名")
    if not path.suffix:
        path = path.with_suffix(".dxf")
    if path.suffix.lower() != ".dxf":
        raise ValueError("目标文件必须使用 .dxf 扩展名")
    return path.absolute()


def output_conflicts(path: Path) -> tuple[Path, ...]:
    """DXF + writer's name.manifest.json preflight, including dangling symlinks."""
    targets = (path, path.with_suffix(".manifest.json"))
    conflicts = []
    for target in targets:
        if target.is_symlink():
            raise ValueError(f"拒绝覆盖符号链接：{target}")
        if target.exists():
            if not target.is_file():
                raise ValueError(f"目标不是普通文件：{target}")
            conflicts.append(target)
    return tuple(conflicts)


def show():
    """Show/reuse the independent dialog inside an already running FreeCAD GUI.

    Does not start FreeCAD, create a QApplication, install anything, or touch a
    source document. Runtime imports intentionally live only in this function.
    """
    global _dialog
    try:
        import FreeCAD
        import FreeCADGui
        from PySide import QtCore
        try:
            from PySide import QtWidgets
        except ImportError:  # FreeCAD builds with the older QtGui widget alias
            from PySide import QtGui as QtWidgets
        try:
            from PySide.QtSvgWidgets import QSvgWidget
        except ImportError:
            from PySide.QtSvg import QSvgWidget
    except ImportError as exc:
        raise RuntimeError("请在已启动的 FreeCAD 桌面中运行 OCSMExport；需要 FreeCADGui 和 PySide QtSvg 组件。") from exc
    if not getattr(FreeCAD, "GuiUp", False) or FreeCADGui.getMainWindow() is None:
        raise RuntimeError("OCSMExport.show() 仅能在已启动的 FreeCAD GUI 内使用。")
    from .api import build_scene, export_scene, options_from_techdraw

    if _dialog is not None:
        try:
            _dialog.show()
            _dialog.raise_()
            _dialog.activateWindow()
            return _dialog
        except RuntimeError:  # a deleted Qt wrapper
            _dialog = None

    class ExportDialog(QtWidgets.QDialog):
        def __init__(self):
            super().__init__(FreeCADGui.getMainWindow())
            self.setWindowTitle("OCSMExport — 选择对象 → 工程视图 DXF")
            self.setWindowFlags(self.windowFlags() | QtCore.Qt.Window)
            self.setAttribute(QtCore.Qt.WA_DeleteOnClose)
            self.setModal(False)
            self.resize(1250, 880)
            self._objects = ()
            self._selection_key = None
            self._busy = False
            self._applying = False
            outer = QtWidgets.QVBoxLayout(self)
            hint = QtWidgets.QLabel("离线优先 · 只处理选中对象/既有 TechDraw 视图 · 不修改模型 · 不自动下载或安装依赖")
            hint.setWordWrap(True)
            outer.addWidget(hint)
            splitter = QtWidgets.QSplitter(QtCore.Qt.Horizontal)
            outer.addWidget(splitter, 1)
            scroll = QtWidgets.QScrollArea()
            scroll.setWidgetResizable(True)
            self.controls = QtWidgets.QWidget()
            left = QtWidgets.QVBoxLayout(self.controls)
            scroll.setWidget(self.controls)
            splitter.addWidget(scroll)

            scope = QtWidgets.QGroupBox("当前选择范围（预览/导出前自动刷新）")
            scope_layout = QtWidgets.QVBoxLayout(scope)
            self.selection_text = QtWidgets.QPlainTextEdit()
            self.selection_text.setReadOnly(True)
            self.selection_text.setMaximumHeight(95)
            scope_layout.addWidget(self.selection_text)
            scope_buttons = QtWidgets.QHBoxLayout()
            refresh = QtWidgets.QPushButton("刷新选择范围")
            refresh.clicked.connect(self._refresh_clicked)
            read_view = QtWidgets.QPushButton("读取所选视图设置")
            read_view.clicked.connect(self._read_view)
            scope_buttons.addWidget(refresh)
            scope_buttons.addWidget(read_view)
            scope_layout.addLayout(scope_buttons)
            left.addWidget(scope)

            views = QtWidgets.QGroupBox("视向与每视图输出（可批量；相机透视不带入导出）")
            view_layout = QtWidgets.QVBoxLayout(views)
            self.view_checks = {}
            self.view_modes = {}
            for key, label in VIEW_LABELS.items():
                row = QtWidgets.QHBoxLayout()
                check = QtWidgets.QCheckBox(label)
                self.view_checks[key] = check
                row.addWidget(check)
                combo = QtWidgets.QComboBox()
                for text, token in (("跟随默认", "inherit"),
                                    ("纯实线", "visible"),
                                    ("实线 + 隐藏虚线", "hidden"),
                                    ("平面全剖视", "section"),
                                    ("全剖 + 隐藏虚线", "section_hidden")):
                    combo.addItem(text, token)
                combo.setToolTip("每个视图独立选择；全剖共用下方剖切平面参数。")
                self.view_modes[key] = combo
                row.addWidget(combo, 1)
                view_layout.addLayout(row)
            self.custom_box = QtWidgets.QWidget()
            custom_layout = QtWidgets.QFormLayout(self.custom_box)
            self.custom_name = QtWidgets.QLineEdit("custom")
            self.direction = QtWidgets.QLineEdit("0, -1, 0")
            self.up = QtWidgets.QLineEdit("0, 0, 1")
            # Text fields preserve imported precision instead of spinbox rounding.
            self.rotation = QtWidgets.QLineEdit("0")
            custom_layout.addRow("视向名称", self.custom_name)
            custom_layout.addRow("指向观察者 (X,Y,Z)", self.direction)
            custom_layout.addRow("上方向 (X,Y,Z)", self.up)
            custom_layout.addRow("平面旋转 (°)", self.rotation)
            view_layout.addWidget(self.custom_box)
            self.view_checks["custom"].toggled.connect(self.custom_box.setVisible)
            left.addWidget(views)

            self.layout_mode = QtWidgets.QComboBox()
            self.layout_mode.addItem("第一角（GB：俯视图在主视图下方，左视图在右侧）", "first_angle")
            self.layout_mode.addItem("第三角（俯视图在主视图上方，右视图在右侧）", "third_angle")
            layout_row = QtWidgets.QFormLayout()
            layout_row.addRow("图样布局", self.layout_mode)
            left.addLayout(layout_row)
            self.advanced_toggle = QtWidgets.QToolButton()
            self.advanced_toggle.setText("高级参数（折叠不重置）")
            self.advanced_toggle.setCheckable(True)
            self.advanced_toggle.setToolButtonStyle(QtCore.Qt.ToolButtonTextBesideIcon)
            self.advanced_toggle.setArrowType(QtCore.Qt.RightArrow)
            left.addWidget(self.advanced_toggle)
            self.advanced = QtWidgets.QWidget()
            advanced = QtWidgets.QFormLayout(self.advanced)
            self.smooth = QtWidgets.QCheckBox("输出可见相切/平滑边")
            self.seams = QtWidgets.QCheckBox("输出可见曲面接缝")
            self.cosmetics = QtWidgets.QCheckBox("包含所选 TechDraw 修饰线/中心线")
            self.cosmetics.setToolTip("本版尚未支持注释纸面坐标映射。默认防丢保护：检测到此类内容则报错。\n"
                                     "只有明确取消勾选，才忽略它们并导出纯模型几何（记录警告）。")
            advanced.addRow(self.smooth)
            advanced.addRow(self.seams)
            self.hidden_hard = QtWidgets.QCheckBox("输出隐藏硬边（仅含隐藏线/剖视隐藏边时生效）")
            self.hidden_smooth = QtWidgets.QComboBox()
            self.hidden_seams = QtWidgets.QComboBox()
            for combo in (self.hidden_smooth, self.hidden_seams):
                combo.addItem("跟随可见边设置", "inherit")
                combo.addItem("独立开启", "on")
                combo.addItem("独立关闭", "off")
            advanced.addRow(self.hidden_hard)
            advanced.addRow("隐藏相切/平滑边", self.hidden_smooth)
            advanced.addRow("隐藏接缝边", self.hidden_seams)
            advanced.addRow(self.cosmetics)
            cosmetics_help = QtWidgets.QLabel("注意：本版尚未支持 TechDraw 修饰线、中心线或 GeomFormats 的纸面坐标映射。"
                                              "默认勾选会在存在此类注释时明确报错；主动取消后仅导出模型几何，并报告忽略警告。")
            cosmetics_help.setWordWrap(True)
            advanced.addRow(cosmetics_help)
            self.tolerance = QtWidgets.QLineEdit("0.01")
            advanced.addRow("误差容限 (mm)", self.tolerance)
            self.approximation = QtWidgets.QCheckBox("允许不支持曲线离散（默认关闭；报告损失）")
            advanced.addRow(self.approximation)
            section_group = QtWidgets.QGroupBox("平面剖切（仅剖视模式生效）")
            section_form = QtWidgets.QFormLayout(section_group)
            self.section_origin = QtWidgets.QLineEdit("0, 0, 0")
            self.section_normal = QtWidgets.QLineEdit("0, -1, 0")
            self.section_keep = QtWidgets.QComboBox()
            self.section_keep.addItem("负侧：normal · (p - origin) ≤ 0", "negative")
            self.section_keep.addItem("正侧：normal · (p - origin) ≥ 0", "positive")
            self.section_hidden = QtWidgets.QCheckBox("剖视也输出隐藏边（默认关闭）")
            self.hatch_angle = QtWidgets.QLineEdit("0")
            self.hatch_spacing = QtWidgets.QLineEdit("3.175")
            self.hatch = QtWidgets.QCheckBox("输出剖面填充（关闭只输出截面轮廓）")
            self.hatch.setChecked(True)
            section_form.addRow("原点 (mm)", self.section_origin)
            center_button = QtWidgets.QPushButton("取选区中心（只读）")
            center_button.clicked.connect(self._section_center)
            section_form.addRow(center_button)
            section_form.addRow("法向 (X,Y,Z)", self.section_normal)
            section_form.addRow("保留侧", self.section_keep)
            section_form.addRow(self.section_hidden)
            section_form.addRow("剖面线角度 (°)", self.hatch_angle)
            section_form.addRow("剖面线间距 (mm)", self.hatch_spacing)
            section_form.addRow(self.hatch)
            section_help = QtWidgets.QLabel("平面必须穿过实体内部。保留负侧时，从法向所指的移除侧观察；"
                                            "保留正侧时反向观察。批量剖视的每个视向都须能看到截面，不能侧向或从保留侧看。")
            section_help.setWordWrap(True)
            section_form.addRow(section_help)
            advanced.addRow(section_group)
            self.writer_python = QtWidgets.QLineEdit()
            self.writer_python.setPlaceholderText("可选：已有 ezdxf 的 Python；空白使用 API 离线查找")
            advanced.addRow("离线写出 Python", self.writer_python)
            offline = QtWidgets.QLabel("仅调用现有依赖；缺依赖时报错，不安装、不联网。复杂 TechDraw 视图明确拒绝。")
            offline.setWordWrap(True)
            advanced.addRow(offline)
            self.advanced.setVisible(False)
            self.advanced_toggle.toggled.connect(self.advanced.setVisible)
            self.advanced_toggle.toggled.connect(lambda checked: self.advanced_toggle.setArrowType(
                QtCore.Qt.DownArrow if checked else QtCore.Qt.RightArrow))
            left.addWidget(self.advanced)
            for combo in self.view_modes.values():
                combo.currentIndexChanged.connect(self._mode_changed)

            target = QtWidgets.QGroupBox("导出文件（模型空间 1:1，mm）")
            target_layout = QtWidgets.QVBoxLayout(target)
            self.path = QtWidgets.QLineEdit()
            self.path.setPlaceholderText("选择目标 .dxf；同时输出审计清单")
            path_row = QtWidgets.QHBoxLayout()
            path_row.addWidget(self.path, 1)
            browse = QtWidgets.QPushButton("选择路径…")
            browse.clicked.connect(self._browse)
            path_row.addWidget(browse)
            target_layout.addLayout(path_row)
            left.addWidget(target)
            left.addStretch()
            right = QtWidgets.QWidget()
            right_layout = QtWidgets.QVBoxLayout(right)
            self.preview_label = QtWidgets.QLabel("SVG 纯预览：点击“预览”生成；不创建 FreeCAD 对象。")
            self.preview_label.setWordWrap(True)
            right_layout.addWidget(self.preview_label)
            self.svg = QSvgWidget()
            self.svg.setMinimumSize(300, 240)
            right_layout.addWidget(self.svg, 3)
            self.report = QtWidgets.QPlainTextEdit()
            self.report.setReadOnly(True)
            self.report.setPlaceholderText("此处报告采样尺寸（mm）、OCSM 图层和警告。真实 GUI/OCS 验收另行进行。")
            right_layout.addWidget(self.report, 2)
            splitter.addWidget(right)
            splitter.setSizes([460, 780])
            buttons = QtWidgets.QHBoxLayout()
            self.preview_button = QtWidgets.QPushButton("预览（纯 SVG）")
            self.export_button = QtWidgets.QPushButton("导出 DXF…")
            close = QtWidgets.QPushButton("关闭")
            self.preview_button.clicked.connect(self._preview)
            self.export_button.clicked.connect(self._export)
            close.clicked.connect(self.close)
            buttons.addWidget(self.preview_button)
            buttons.addWidget(self.export_button)
            buttons.addStretch()
            buttons.addWidget(close)
            outer.addLayout(buttons)
            # Prevent Return in a parameter field from inadvertently exporting.
            for button in self.findChildren(QtWidgets.QPushButton):
                button.setAutoDefault(False)
            for check in self.view_checks.values():
                check.toggled.connect(self._invalidate)
            for check in (self.smooth, self.seams, self.cosmetics, self.approximation, self.section_hidden, self.hidden_hard, self.hatch):
                check.toggled.connect(self._invalidate)
            for edit in (self.custom_name, self.direction, self.up, self.rotation, self.tolerance,
                         self.section_origin, self.section_normal, self.hatch_angle, self.hatch_spacing):
                edit.textChanged.connect(self._invalidate)
            self.layout_mode.currentIndexChanged.connect(self._invalidate)
            for combo in (self.section_keep, self.hidden_smooth, self.hidden_seams):
                combo.currentIndexChanged.connect(self._invalidate)
            for combo in self.view_modes.values():
                combo.currentIndexChanged.connect(self._invalidate)
            self._apply_options(ExportOptions())
            self._refresh_selection()
            # Read-only monitoring invalidates snapshots when the selection changes.
            self.timer = QtCore.QTimer(self)
            self.timer.setInterval(700)
            self.timer.timeout.connect(self._poll_selection)
            self.timer.start()

        def _invalidate(self, *_):
            if self._applying:
                return
            self.svg.hide()
            self.preview_label.setText("预览未生成/已过期：按当前选择和参数重新点击“预览”。导出总是重新计算。")

        def _mode_changed(self, *_):
            if any(combo.currentData() in ("section", "section_hidden")
                   for combo in self.view_modes.values()):
                self.advanced_toggle.setChecked(True)

        def _message(self, title, text, *, question=False, details=None):
            message = QtWidgets.QMessageBox(self)
            message.setWindowTitle(title)
            message.setTextFormat(QtCore.Qt.PlainText)
            message.setText(str(text))
            if question:
                message.setIcon(QtWidgets.QMessageBox.Warning)
                message.setStandardButtons(QtWidgets.QMessageBox.Yes | QtWidgets.QMessageBox.No)
                message.setDefaultButton(QtWidgets.QMessageBox.No)
            else:
                message.setIcon(QtWidgets.QMessageBox.Critical)
                message.setStandardButtons(QtWidgets.QMessageBox.Ok)
            if details:
                message.setDetailedText(details)
            execute = getattr(message, "exec", None) or message.exec_
            return execute() == QtWidgets.QMessageBox.Yes

        def _error(self, exc):
            self._invalidate()
            text = f"操作失败：{type(exc).__name__}: {exc}\n未报告导出成功；如有目标文件，请以本次写出报告为准。"
            if self.cosmetics.isChecked():
                text += ("\n若所选 TechDraw 含修饰线/中心线/GeomFormats：本版尚未支持其注释映射，默认防丢会拒绝。"
                         "确认接受忽略后，可在高级参数中主动取消“包含所选 TechDraw 修饰线/中心线”，仅导出模型几何。")
            self.report.setPlainText(text)
            self._message("OCSMExport — 操作失败", text, details=traceback.format_exc())

        def _refresh_selection(self):
            records = list(FreeCADGui.Selection.getSelectionEx())
            objects, names, keys = [], [], []
            seen = set()
            for record in records:
                obj = record.Object
                document = getattr(getattr(obj, "Document", None), "Name", "?")
                name = getattr(obj, "Name", "?")
                subnames = tuple(getattr(record, "SubElementNames", ()))
                key = (document, name)
                keys.append((key, subnames))
                if key in seen:
                    continue
                seen.add(key)
                objects.append(obj)
                label = getattr(obj, "Label", name)
                names.append(f"{document}.{name} — {label}")
                if subnames:
                    names.append("  子元素选择按完整对象导出（不只导出所选面/边）：" + ", ".join(subnames))
            selection_key = tuple(keys)
            if selection_key != self._selection_key:
                self._selection_key = selection_key
                self._objects = tuple(objects)
                self.selection_text.setPlainText("\n".join(names) or "未选择对象：不会默认导出整个文档。")
                self._invalidate()
            else:
                self._objects = tuple(objects)  # refresh wrappers after document changes
            return self._objects

        def _poll_selection(self):
            if not self._busy:
                try:
                    self._refresh_selection()
                except Exception as exc:
                    # No repeated modal errors from a timer (e.g. a closing document).
                    self._objects = ()
                    self._selection_key = None
                    self.selection_text.setPlainText(f"无法读取选择：{exc}；请刷新。")
                    self._invalidate()

        def _refresh_clicked(self, *_):
            try:
                self._refresh_selection()
                self._invalidate()  # also refreshes after in-place geometry edits
            except Exception as exc:
                self._error(exc)

        def _inputs(self):
            objects = self._refresh_selection()
            if not objects:
                raise ExportError("请选择至少一个模型对象，或一个支持的 TechDraw 视图。")
            values = self._form()
            direction = up = None
            if "camera" in values["views"]:
                gui_doc = FreeCADGui.activeDocument()
                if gui_doc is None:
                    raise ExportError("没有活动三维文档，不能读取当前相机。")
                view = gui_doc.activeView()
                if not hasattr(view, "getViewDirection") or not hasattr(view, "getCameraOrientation"):
                    raise ExportError("当前不是可读取相机的三维视图，请选择正投影或自定义方向。")
                direction = tuple(view.getViewDirection())
                up = tuple(view.getCameraOrientation().multVec(FreeCAD.Vector(0, 1, 0)))
            return objects, options_from_form(values, camera_direction=direction, camera_up=up)

        def _form(self):
            return dict(views=[key for key, check in self.view_checks.items() if check.isChecked()],
                        view_modes={key: combo.currentData() for key, combo in self.view_modes.items()},
                        mode="visible", layout=self.layout_mode.currentData(), custom_name=self.custom_name.text(),
                        direction=self.direction.text(), up=self.up.text(), rotation_deg=self.rotation.text(),
                        include_smooth=self.smooth.isChecked(), include_seams=self.seams.isChecked(),
                        include_cosmetics=self.cosmetics.isChecked(), tolerance=self.tolerance.text(),
                        hidden_hard=self.hidden_hard.isChecked(),
                        hidden_smooth={"inherit": None, "on": True, "off": False}[self.hidden_smooth.currentData()],
                        hidden_seams={"inherit": None, "on": True, "off": False}[self.hidden_seams.currentData()],
                        allow_approximation=self.approximation.isChecked(),
                        section_origin=self.section_origin.text(), section_normal=self.section_normal.text(),
                        section_keep=self.section_keep.currentData(), section_hidden=self.section_hidden.isChecked(),
                        hatch_angle_deg=self.hatch_angle.text(), hatch_spacing=self.hatch_spacing.text(),
                        hatch_enabled=self.hatch.isChecked())

        def _apply_options(self, options):
            form = form_from_options(options)  # validate representation before changing UI
            self._applying = True
            try:
                for key, check in self.view_checks.items():
                    check.setChecked(key in form["views"])
                self.custom_box.setVisible("custom" in form["views"])
                self.custom_name.setText(form["custom_name"])
                for widget, key in ((self.direction, "direction"), (self.up, "up"),
                                    (self.section_origin, "section_origin"), (self.section_normal, "section_normal")):
                    widget.setText(", ".join(repr(v) for v in form[key]))
                for widget, key in ((self.rotation, "rotation_deg"), (self.tolerance, "tolerance"),
                                    (self.hatch_angle, "hatch_angle_deg"), (self.hatch_spacing, "hatch_spacing")):
                    widget.setText(repr(form[key]))
                for widget, key in ((self.smooth, "include_smooth"), (self.seams, "include_seams"),
                                    (self.cosmetics, "include_cosmetics"), (self.approximation, "allow_approximation"),
                                    (self.section_hidden, "section_hidden"), (self.hidden_hard, "hidden_hard"),
                                    (self.hatch, "hatch_enabled")):
                    widget.setChecked(form[key])
                for widget, key in ((self.hidden_smooth, "hidden_smooth"), (self.hidden_seams, "hidden_seams")):
                    widget.setCurrentIndex(widget.findData({None: "inherit", True: "on", False: "off"}[form[key]]))
                self.layout_mode.setCurrentIndex(self.layout_mode.findData(form["layout"]))
                for key, combo in self.view_modes.items():
                    combo.setCurrentIndex(combo.findData(form["view_modes"].get(key, "inherit")))
                self.section_keep.setCurrentIndex(self.section_keep.findData(form["section_keep"]))
            finally:
                self._applying = False
            self._invalidate()

        def _section_center(self, *_):
            try:
                objects = self._refresh_selection()
                center = selection_center(objects)
                self.section_origin.setText(", ".join(repr(value) for value in center))
                self.report.setPlainText(f"已将剖切原点设为选区包围盒中心 {center} mm。请核对法向、保留侧与视向。未修改源模型。")
            except Exception as exc:
                self._error(exc)

        def _read_view(self, *_):
            try:
                objects = self._refresh_selection()
                if len(objects) != 1:
                    raise ExportError("读取设置需要且只允许选择一个普通/平面剖切 TechDraw 视图。")
                options = options_from_techdraw(objects[0])
                self._apply_options(options)
                self.advanced_toggle.setChecked(True)
                self.report.setPlainText("已应用所选 TechDraw 视图的视向/上方向/旋转、独立的可见/隐藏边分类开关与剖切设置。\n"
                                         "纸面缩放不带入导出；请核对参数后预览。未改变所选视图或源模型。\n"
                                         "注意：本版尚未支持修饰线/中心线/GeomFormats 的注释映射。默认包含时防丢报错；"
                                         "主动取消包含后仅导出模型几何，并报告忽略警告。")
            except Exception as exc:
                self._error(exc)

        def _browse(self, *_):
            result = QtWidgets.QFileDialog.getSaveFileName(
                self, "选择 DXF 路径（导出时单独确认覆盖）", self.path.text() or "OCSM-export.dxf",
                "DXF (*.dxf)", options=QtWidgets.QFileDialog.DontConfirmOverwrite)
            filename = result[0] if isinstance(result, tuple) else result
            if filename:
                self.path.setText(filename)

        def _set_busy(self, busy):
            self._busy = busy
            self.controls.setEnabled(not busy)
            self.preview_button.setEnabled(not busy)
            self.export_button.setEnabled(not busy)
            if busy:
                QtWidgets.QApplication.setOverrideCursor(QtCore.Qt.WaitCursor)
            else:
                QtWidgets.QApplication.restoreOverrideCursor()

        def _preview(self, *_):
            try:
                objects, options = self._inputs()
                self._invalidate()
                self._set_busy(True)
                scene = build_scene(objects, options)
                svg = scene_svg(scene)
                summary = scene_summary(scene)
                self.svg.load(QtCore.QByteArray(svg.encode("utf-8")))
                if not self.svg.renderer().isValid():
                    raise ExportError("Qt SVG 渲染器拒绝本次预览；未写出 DXF。")
                self.svg.show()
                self.preview_label.setText("SVG 采样快照（mm，按场景已有图层绘制）；不含未支持的 TechDraw 注释映射。导出不复用此快照。")
                self.report.setPlainText(summary)
            except Exception as exc:
                self._error(exc)
            finally:
                if self._busy:
                    self._set_busy(False)

        def _confirm_overwrite(self, path, reason=""):
            return self._message("确认覆盖已有导出", f"目标：{path}\n{reason}\n"
                                 "已有 DXF 及关联清单可能被替换，且不能由 FreeCAD 撤销。是否明确允许覆盖？",
                                 question=True)

        def _export(self, *_):
            try:
                objects, options = self._inputs()
                path = target_path(self.path.text())
                if not path.parent.is_dir():
                    raise ValueError(f"目标目录不存在，请选择已有目录：{path.parent}")
                overwrite = False
                conflicts = output_conflicts(path)
                if conflicts:
                    if not self._confirm_overwrite(path, "已存在：\n" + "\n".join(str(p) for p in conflicts)):
                        self.report.setPlainText("已取消导出，未写入文件。")
                        return
                    overwrite = True
                self._invalidate()
                self._set_busy(True)
                # Never export a cached preview, including after document geometry changes.
                scene = build_scene(objects, options)
                summary = scene_summary(scene)
                executable = self.writer_python.text().strip() or None
                try:
                    result = export_scene(scene, path, overwrite=overwrite, python_executable=executable)
                except FileExistsError as exc:
                    # Do not confuse a writer lock with overwrite permission.
                    if not output_conflicts(path):
                        raise ExportError(f"写出文件被占用/锁定，请确认没有其他写出进程；不会绕过锁：{exc}") from exc
                    if not self._confirm_overwrite(path, str(exc)):
                        self.report.setPlainText("检测到文件冲突，已取消；未报告导出成功。")
                        return
                    objects, options = self._inputs()
                    scene = build_scene(objects, options)
                    summary = scene_summary(scene)
                    result = export_scene(scene, path, overwrite=True, python_executable=executable)
                lines = [summary, "", "DXF 写出成功（尚不代表 OCS/真实 GUI 验收通过）", f"文件：{result['path']}",
                         f"审计清单：{result['manifest_path']}", f"实体统计：{result['counts']}"]
                lines.extend("写出警告：" + str(w) for w in result.get("warnings", []))
                self.report.setPlainText("\n".join(lines))
            except Exception as exc:
                self._error(exc)
            finally:
                if self._busy:
                    self._set_busy(False)

    def released(*_):
        global _dialog
        _dialog = None

    _dialog = ExportDialog()
    _dialog.destroyed.connect(released)
    _dialog.show()
    return _dialog
