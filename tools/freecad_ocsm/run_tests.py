#!/usr/bin/env python3
"""Run Python tests and isolated FreeCADCmd tests; never starts a CAD GUI/MCP."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main(argv=None) -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--python-only',action='store_true',help='Skip tests requiring FreeCADCmd')
    parser.add_argument('--examples',action='store_true',help='Also regenerate synthetic FCStd/IR/DXF examples')
    parser.add_argument('--native-baseline',action='store_true',help='Also compare the built-in TechDraw DXF path')
    parser.add_argument('--freecad-cmd',default=os.environ.get('FREECAD_CMD','FreeCADCmd'))
    parser.add_argument('--ocs-executable',type=Path,
                        help='Optional existing OCS binary for isolated DXF codec save/reopen checks')
    args=parser.parse_args(argv)
    root=Path(__file__).resolve().parent
    env=os.environ.copy()
    env['PYTHONPATH']=str(root)+os.pathsep+env.get('PYTHONPATH','')
    env['QT_QPA_PLATFORM']='offscreen'
    commands=[[sys.executable,'-m','unittest','discover','-s','tests','-p','test_*.py','-v']]
    if not args.python_only:
        executable=shutil.which(args.freecad_cmd)
        if executable is None:
            parser.error('FreeCADCmd not found; set --freecad-cmd or choose --python-only')
        commands.append([executable,str(root/'tests'/'run_freecad.FCMacro')])
        if args.native_baseline:
            commands.append([executable,str(root/'tests'/'probe_native.FCMacro')])
        if args.examples:
            commands.append([executable,str(root/'tests'/'export_examples.FCMacro')])
    elif args.examples or args.native_baseline:
        parser.error('--examples/--native-baseline require FreeCADCmd (not --python-only)')
    if args.ocs_executable:
        commands.append([sys.executable,str(root/'tests'/'validate_ocs.py'),
                         '--ocs-executable',str(args.ocs_executable.expanduser().resolve())])
    for command in commands:
        print('+ '+' '.join(command),flush=True)
        try:
            result=subprocess.run(command,cwd=root,env=env,timeout=180,check=False)
        except (OSError,subprocess.TimeoutExpired) as exc:
            print(f'Test process failed: {exc}',file=sys.stderr)
            return 1
        if result.returncode:
            return result.returncode
    print('All requested independent-process tests passed. Live CAD GUI acceptance remains separate.')
    return 0


if __name__=='__main__':
    raise SystemExit(main())
