#!/usr/bin/env python3
"""Enforce deterministic Rust source limits without third-party dependencies."""
import argparse
from pathlib import Path
import sys


def violations(root: Path, max_lines: int, max_width: int) -> list[str]:
    problems = []
    for path in sorted(root.rglob('*.rs')):
        if any(part in {'target', '.git', 'vendor'} for part in path.parts):
            continue
        content = path.read_text(encoding='utf-8')
        lines = content.splitlines()
        relative = path.relative_to(root)
        if len(lines) > max_lines:
            problems.append(f'{relative}: {len(lines)} lines exceeds {max_lines}')
        for number, line in enumerate(lines, 1):
            if len(line) > max_width:
                problems.append(f'{relative}:{number}: {len(line)} chars exceeds {max_width}')
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--max-lines', type=int, default=600)
    parser.add_argument('--max-width', type=int, default=120)
    args = parser.parse_args()
    problems = violations(args.root, args.max_lines, args.max_width)
    for problem in problems:
        print(problem)
    if problems:
        print(f'FAIL: {len(problems)} source limit violations', file=sys.stderr)
        return 1
    print('PASS: Rust source limits')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
