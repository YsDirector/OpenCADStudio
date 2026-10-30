"""FreeCAD startup hook: only make this plugin's package importable."""
from pathlib import Path
import sys

# FreeCAD exec(compile(..., InstallFile, 'exec')) may not supply __file__.
_ocsm_plugin_dir = str(Path(sys._getframe().f_code.co_filename).resolve().parent)
if _ocsm_plugin_dir not in sys.path:
    sys.path.insert(0, _ocsm_plugin_dir)
