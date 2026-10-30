"""FreeCAD workbench registration, following the built-in Python WB pattern.

No geometry/writer imports at startup. FreeCAD supplies the Workbench base;
see the installed Mod/OpenSCAD/InitGui.py Initialize/GetClassName example.
"""
import FreeCADGui as Gui


class _OCSMExportSelection:
    def GetResources(self):
        return {"MenuText": "OCSMExport — 导出所选对象…",
                "ToolTip": "离线导出选中模型或 TechDraw 视图；独立 SVG 预览，不修改源文档"}

    def IsActive(self):
        import FreeCAD
        return FreeCAD.ActiveDocument is not None

    def Activated(self):
        try:
            from ocsm_export.gui import show
            show()
        except Exception as exc:
            import FreeCAD
            from PySide import QtCore
            try:
                from PySide import QtWidgets
            except ImportError:
                from PySide import QtGui as QtWidgets
            text = f"无法打开 OCSMExport：{type(exc).__name__}: {exc}\n请确认完整插件目录已放入 FreeCAD 用户 Mod 目录。"
            FreeCAD.Console.PrintError(text + "\n")
            message = QtWidgets.QMessageBox(Gui.getMainWindow())
            message.setWindowTitle("OCSMExport")
            message.setTextFormat(QtCore.Qt.PlainText)
            message.setIcon(QtWidgets.QMessageBox.Critical)
            message.setText(text)
            execute = getattr(message, "exec", None) or message.exec_
            execute()


class OCSMExport(Workbench):
    MenuText = "OCSMExport"
    ToolTip = "选择对象 → OCSM 工程图层 DXF（离线优先）"

    def Initialize(self):
        self.appendToolbar("OCSMExport", ["OCSMExport"])
        self.appendMenu("OCSMExport", ["OCSMExport"])

    def GetClassName(self):
        return "Gui::PythonWorkbench"


Gui.addCommand("OCSMExport", _OCSMExportSelection())
Gui.addWorkbench(OCSMExport())
