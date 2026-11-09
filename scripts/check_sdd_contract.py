#!/usr/bin/env python3
"""Validate M3.1 SDD traceability without treating proposed docs as approved."""
from pathlib import Path
import re
import sys

REQUIREMENTS = "docs/requirements/m31-worker-supervision.md"
DESIGN = "docs/design/m31-worker-supervision.md"
MATRIX = "docs/requirements/m31-acceptance-matrix.md"
SPEC = "openspec/specs/m31-tokio-supervisor.md"
GOVERNANCE = "docs/development/requirements-and-design.md"
IDS = re.compile(r"\bM31-(?:R|N)\d+\b")
DEFINITION = re.compile(r"^\s*-\s*\*\*(M31-(?:R|N)\d+)\*\*", re.MULTILINE)
MATRIX_ROW = re.compile(r"^\|\s*(M31-(?:R|N)\d+)\s*\|", re.MULTILINE)


def validate(root: Path) -> list[str]:
    errors = []
    docs = {}
    for path in (REQUIREMENTS, DESIGN, MATRIX, SPEC, GOVERNANCE):
        file = root / path
        if not file.is_file():
            errors.append(f"missing SDD artifact: {path}")
        else:
            docs[path] = file.read_text(encoding="utf-8")
    if len(docs) != 5:
        return errors
    requirements = DEFINITION.findall(docs[REQUIREMENTS])
    rows = MATRIX_ROW.findall(docs[MATRIX])
    if len(requirements) != len(set(requirements)):
        errors.append("duplicate requirement definitions")
    if len(rows) != len(set(rows)):
        errors.append("duplicate acceptance matrix rows")
    expected = set(requirements)
    if not expected:
        errors.append("no requirement definitions")
    for path, present in ((DESIGN, set(IDS.findall(docs[DESIGN]))),
                          (MATRIX, set(rows))):
        for missing in sorted(expected - present):
            errors.append(f"{path}: missing {missing}")
        for extra in sorted(present - expected):
            errors.append(f"{path}: undefined {extra}")
    if "approval pending" not in docs[DESIGN].lower() and "approval required" not in docs[DESIGN].lower():
        errors.append("design approval status must be explicit")
    if "maintainer agreement required" not in docs[REQUIREMENTS].lower() and "approved" not in docs[REQUIREMENTS].lower():
        errors.append("requirements approval status must be explicit")
    for path in (REQUIREMENTS, DESIGN):
        if "openspec/specs/m31-tokio-supervisor.md" not in docs[path]:
            errors.append(f"{path}: missing OpenSpec link")
    if "docs/requirements/m31-worker-supervision.md" not in docs[SPEC]:
        # Relative links are allowed and preferred.
        if "m31-worker-supervision.md" not in docs[SPEC]:
            errors.append("OpenSpec missing requirements link")
    return errors


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    errors = validate(root)
    for error in errors:
        print(f"FAIL: {error}")
    if not errors:
        print("PASS: SDD requirements/design/acceptance traceability")
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(main())
