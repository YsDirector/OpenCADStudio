#!/usr/bin/env python3
"""Explicit, non-destructive user-level installation of the FreeCAD workbench."""
from __future__ import annotations

import argparse
from pathlib import Path
import shutil


def install(mod_dir: Path, *, copy: bool = False, dry_run: bool = False) -> Path:
    source = Path(__file__).resolve().parent
    destination = mod_dir.expanduser().resolve() / 'OCSMExport'
    if destination.is_symlink() and destination.resolve() == source:
        return destination
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(f'Refusing to replace existing addon: {destination}')
    if not dry_run:
        destination.parent.mkdir(parents=True, exist_ok=True)
        if copy:
            shutil.copytree(source, destination,
                            ignore=shutil.ignore_patterns('.venv', '__pycache__', '*.pyc', 'artifacts', '*.egg-info'))
        else:
            destination.symlink_to(source, target_is_directory=True)
    return destination


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mod-dir', type=Path, required=True,
                        help='FreeCAD user data Mod directory, e.g. ~/.local/share/FreeCAD/v1-1/Mod')
    parser.add_argument('--copy', action='store_true', help='Copy instead of symlink (e.g. Windows)')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args(argv)
    try:
        path = install(args.mod_dir, copy=args.copy, dry_run=args.dry_run)
    except (OSError, ValueError) as exc:
        parser.exit(1, f'{exc}\n')
    print(f'{"Would install" if args.dry_run else "Installed"}: {path}')
    print('No dependencies were installed. Restart FreeCAD to load the OCSM Export workbench.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
