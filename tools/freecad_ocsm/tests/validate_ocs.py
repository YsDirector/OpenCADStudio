#!/usr/bin/env python3
"""Optional OCS headless codec check, never a live GUI/import acceptance test.

Uses an existing OCS binary with --serve on stdin (no sockets/MCP/plugins).
Only generated synthetic DXFs are opened. Copies and logs go to a fresh
artifacts subdirectory. No build, software installation or user drawing edits.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
from typing import Any

import ezdxf
from ezdxf.lldxf.tagwriter import TagCollector

from ocsm_export.model import LAYERS


def normalize(value: Any) -> Any:
    if isinstance(value, float):
        if not math.isfinite(value):
            raise AssertionError('Nonfinite DXF geometry')
        return round(value, 8) or 0.0
    if isinstance(value, (str, int)) or value is None:
        return value
    return [normalize(v) for v in value]


def entity_signature(entity) -> list:
    # Compare geometry, styles, patterns and loops, not just entity counts.
    # Database identity/ownership may change. OCS also writes absent SPLINE
    # fitting tolerances as zero: check/report that known metadata delta below,
    # while still requiring degree/flags/poles/knots/weights to be unchanged.
    ignored = {5, 330}
    if entity.dxftype() == 'SPLINE':
        ignored.update((42, 43, 44))
    return [[tag.code, normalize(tag.value)] for tag in
            TagCollector.dxftags(entity, dxfversion='AC1027') if tag.code not in ignored]


def collection_signature(entities) -> list:
    result = []
    for entity in entities:
        assert entity.dxf.color == 256, f'{entity.dxftype()} color is not ByLayer'
        assert entity.dxf.linetype.upper() == 'BYLAYER', 'Entity linetype is not ByLayer'
        assert entity.dxf.lineweight == -1, 'Entity lineweight is not ByLayer'
        result.append(entity_signature(entity))
    return sorted(result, key=lambda item: json.dumps(item, ensure_ascii=False))


def snapshot(path: Path, *, host_notes: list | None = None) -> dict:
    doc = ezdxf.readfile(path)
    auditor = doc.audit()
    assert not auditor.errors, f'ezdxf audit errors: {path}'
    # OCS 2026.42 adds an unrelated TABLESTYLE with owner #0 on save. Record
    # this known host metadata defect, never waive geometry/entity repairs.
    for fix in auditor.fixes:
        known_host_issue = (int(fix.code) == 202 and re.fullmatch(
            r'Deleted TABLESTYLE\(#[0-9A-Fa-f]+\) entity with invalid owner handle #0\.', fix.message))
        assert host_notes is not None and known_host_issue, f'Unexpected ezdxf repair: {fix.message}: {path}'
        host_notes.append({'code': int(fix.code), 'message': fix.message,
                           'scope': 'OCS-added TABLESTYLE metadata; no geometry repair permitted'})
    assert doc.header.get('$INSUNITS') == 4, 'Units are not millimetres'
    layers = {}
    for name, color, linetype, width in LAYERS.values():
        layer = doc.layers.get(name)
        actual = (layer.dxf.color, layer.dxf.linetype.upper(), layer.dxf.lineweight)
        assert actual == (color, linetype.upper(), width), f'Layer style changed: {name}: {actual}'
        layers[name] = actual
    zero_tolerance_splines = 0
    splines = []
    for entity in doc.entitydb.values():
        if not entity.is_alive or entity.dxftype() != 'SPLINE':
            continue
        keys = ('knot_tolerance', 'control_point_tolerance', 'fit_tolerance')
        present = [entity.dxf.hasattr(key) for key in keys]
        values = [normalize(getattr(entity.dxf, key)) for key in keys]
        if any(present):
            # Only the known "absent becomes zero" host delta is tolerated.
            assert all(value == 0.0 for value in values), f'Unexpected SPLINE fitting tolerances: {values}'
            zero_tolerance_splines += 1
        # Canonicalise an absent (ezdxf default) triple with explicit zeros so
        # the two reads compare equal; the saved tag delta is reported instead.
        splines.append([value if present[i] else 0.0 for i, value in enumerate(values)])
    if zero_tolerance_splines and host_notes is not None:
        host_notes.append({'scope': 'OCS SPLINE metadata delta', 'count': zero_tolerance_splines,
                           'message': 'Absent default fitting tolerances were written as zero; '
                                      'degree, flags, poles, knots and weights compared unchanged'})
    linetypes = {name: [[tag.code, normalize(tag.value)] for tag in
                        doc.linetypes.get(name).pattern_tags.tags if tag.code in (40, 49)]
                 for name in ('CENTER2', 'DASHED2')}
    blocks = {block.name: collection_signature(block) for block in doc.blocks
              if block.name.startswith('OCSM_VIEW_')}
    return {'units': 4, 'layers': layers, 'linetypes': linetypes, 'splines': sorted(splines),
            'modelspace': collection_signature(doc.modelspace()), 'blocks': blocks}


def validate(binary: Path, examples: Path, output_root: Path) -> dict:
    files = sorted(examples.glob('*.dxf'))
    if not files:
        raise ValueError(f'No synthetic DXFs in {examples}; run run_tests.py --examples first')
    output_root.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='ocs-codec-', dir=output_root))
    env = os.environ.copy()
    # Keep the new process separate from desktop preferences and plugins.
    env.update(OCS_PLUGINS_DIR=str(work / 'no-user-plugins'),
               OCS_BUNDLED_PLUGINS_DIR=str(work / 'no-bundled-plugins'),
               XDG_CONFIG_HOME=str(work / 'config'), XDG_DATA_HOME=str(work / 'data'),
               XDG_CACHE_HOME=str(work / 'cache'))
    report = {'scope': 'headless codec only; GUI rendering and import into existing drawings NOT tested',
              'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'output_directory': str(work), 'examples': []}
    for source in files:
        target = work / source.name
        requests = [{'op': 'open', 'path': str(source)},
                    {'op': 'audit', 'target_format': 'dxf', 'target_version': '2013'},
                    {'op': 'query', 'detail': 'full', 'limit': 10000},
                    {'op': 'save', 'path': str(target), 'target_format': 'dxf', 'target_version': '2013'},
                    {'op': 'open', 'path': str(target)},
                    {'op': 'query', 'detail': 'full', 'limit': 10000},
                    {'op': 'audit', 'target_format': 'dxf', 'target_version': '2013'}]
        process = subprocess.run([str(binary), '--backend', 'gl', '--serve'],
                                 input='\n'.join(json.dumps(r) for r in requests) + '\n',
                                 text=True, encoding='utf-8', capture_output=True,
                                 env=env, cwd=work, timeout=45, check=False)
        (work / (source.stem + '.jsonl')).write_text(process.stdout, encoding='utf-8')
        (work / (source.stem + '.stderr.log')).write_text(process.stderr, encoding='utf-8')
        assert process.returncode == 0, f'OCS failed ({source.name}): {process.stderr}'
        responses = [json.loads(line) for line in process.stdout.splitlines() if line.strip()]
        assert len(responses) == len(requests) + 1, 'Incomplete OCS response stream'
        assert responses[0].get('ready') is True, 'OCS did not send a ready greeting'
        assert all(r.get('ok') is True for r in responses), f'OCS reported an error: {responses}'
        for index in (2, 7):
            audit = responses[index]
            assert audit.get('status') == 'passed', f'OCS audit failed: {audit}'
            assert audit['summary']['errors'] == 0, f'OCS errors: {audit}'
            assert audit['target']['dropped_on_save'] == 0, f'Lossy OCS save: {audit}'
        assert responses[1]['by_type'] == responses[5]['by_type'], 'OCS entity types changed'
        assert responses[2]['manifest'] == responses[7]['manifest'], 'OCS manifest changed'
        host_notes = []
        assert snapshot(source) == snapshot(target, host_notes=host_notes), f'Geometry/styles changed on OCS roundtrip: {source.name}'
        report['ocs_version'] = responses[0]['version']
        report['examples'].append({'name': source.stem, 'status': 'passed',
                                   'manifest': responses[7]['manifest'], 'host_metadata_notes': host_notes,
                                   'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
        print(f'OCS codec OK: {source.name}', flush=True)
    report['status'] = 'passed'
    (output_root / 'ocs-validation.json').write_text(
        json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    return report


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ocs-executable', required=True, type=Path)
    root = Path(__file__).resolve().parents[1]
    parser.add_argument('--examples', type=Path, default=root / 'artifacts' / 'examples')
    parser.add_argument('--output', type=Path, default=root / 'artifacts')
    args = parser.parse_args(argv)
    binary = args.ocs_executable.expanduser().resolve()
    if not binary.is_file():
        parser.error(f'Existing OCS binary not found: {binary}')
    report = validate(binary, args.examples.resolve(), args.output.resolve())
    print(f"{len(report['examples'])} synthetic files passed OCS {report['ocs_version']} codec roundtrip. "
          'Live GUI/import acceptance remains deferred.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
