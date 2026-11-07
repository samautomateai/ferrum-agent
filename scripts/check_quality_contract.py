#!/usr/bin/env python3
"""Validate that local and CI quality policy cannot silently drift."""
from pathlib import Path
import sys

REQUIRED_TASKS = (
    'source-limits', 'format', 'lint', 'test', 'coverage',
    'dependencies', 'secrets', 'workflow-security', 'build',
)


def validate(root: Path) -> list[str]:
    errors = []
    taskfile = root / 'Taskfile.yml'
    workflow = root / '.github/workflows/quality.yml'
    cargo = root / 'Cargo.lock'
    if not taskfile.is_file():
        return ['Taskfile.yml missing']
    if not workflow.is_file():
        return ['quality.yml missing']
    task = taskfile.read_text(encoding='utf-8')
    ci = workflow.read_text(encoding='utf-8')
    for name in REQUIRED_TASKS:
        if f'  {name}:\n' not in task:
            errors.append(f'missing task: {name}')
    if 'task quality' not in ci:
        errors.append('CI must execute task quality (same local entry point)')
    if 'task: coverage' not in task or '--fail-under-lines 90' not in task:
        errors.append('90% line coverage must be enforced')
    if 'cargo clippy --locked' not in task or '-D warnings' not in task:
        errors.append('Clippy warnings must be errors')
    if 'cargo deny --locked' not in task or 'cargo audit' not in task:
        errors.append('dependency and advisory checks required')
    if 'gitleaks git' not in task:
        errors.append('secret scanning required')
    if 'zizmor' not in task or 'actionlint' not in task:
        errors.append('workflow scanning required')
    if not cargo.is_file():
        errors.append('Cargo.lock missing; --locked commands cannot run')
    return errors


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    problems = validate(root)
    for problem in problems:
        print(f'FAIL: {problem}')
    if not problems:
        print('PASS: quality contract')
    return int(bool(problems))


if __name__ == '__main__':
    sys.exit(main())
