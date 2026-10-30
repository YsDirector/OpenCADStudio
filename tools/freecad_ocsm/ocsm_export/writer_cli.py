"""JSON-only external-Python bridge: python -m ocsm_export.writer_cli ..."""
from __future__ import annotations

import argparse
from contextlib import redirect_stdout
import json
from pathlib import Path
import sys


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Write a schema-v1 OCSM scene as native DXF R2013")
    parser.add_argument("scene", type=Path, help="UTF-8 scene JSON")
    parser.add_argument("output", type=Path, help="output DXF (parent directory must exist)")
    parser.add_argument("--overwrite", action="store_true", help="replace DXF and its manifest transactionally")
    args = parser.parse_args(argv)
    try:
        # Import inside the error boundary so a missing ezdxf also produces a
        # concise bridge error. Library chatter must never corrupt stdout JSON.
        with redirect_stdout(sys.stderr):
            from .dxf_writer import write_dxf

            with args.scene.open("r", encoding="utf-8") as stream:
                scene = json.load(stream)
            report = write_dxf(scene, args.output, overwrite=args.overwrite)
        payload = json.dumps(report, ensure_ascii=False, allow_nan=False)
        data = (payload + "\n").encode("utf-8")
        # Write bytes directly so a non-UTF-8 stdio encoding (Windows redirect,
        # PYTHONIOENCODING=ascii) cannot fail after the file was committed.
        buffer = getattr(sys.stdout, "buffer", None)
        if buffer is not None:
            buffer.write(data)
            buffer.flush()
        else:
            sys.stdout.write(payload + "\n")
            sys.stdout.flush()
    except Exception as exc:
        print(json.dumps({"error": str(exc), "type": type(exc).__name__}, ensure_ascii=False), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
