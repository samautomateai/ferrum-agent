#!/usr/bin/env python3
"""Validate checked-in agent roles and policy; no dependency on Pi runtime."""
from pathlib import Path
import sys

ROLES = ("architect", "scrutinizer", "implementer", "reviewer", "security", "quality")
REQUIRED = (
    "AGENTS.md",
    "docs/agentic/README.md",
    "openspec/specs/agentic-pi-workflow.md",
)
PROMPTS = ("architect", "scrutinize", "implement", "review", "security-review", "quality-review")
POLICY_TERMS = ("OpenSpec", "task quality", "90%", "600", "120")
REVIEW_TERMS = {
    "architect": ("ADR", "approval"),
    "scrutinizer": ("failure", "Do not implement"),
    "implementer": ("failing", "task quality"),
    "reviewer": ("fresh session", "Do not change code"),
    "security": ("separate fresh context", "Do not silently fix"),
    "quality": ("CI", "coverage", "Never"),
}


def validate(root: Path) -> list[str]:
    errors = []
    for path in REQUIRED:
        if not (root / path).is_file():
            errors.append(f"missing agentic artifact: {path}")
    policy = root / "AGENTS.md"
    if policy.is_file():
        content = policy.read_text(encoding="utf-8")
        for term in POLICY_TERMS:
            if term not in content:
                errors.append(f"AGENTS.md missing policy: {term}")
    for role in ROLES:
        path = f"docs/agentic/roles/{role}.md"
        file = root / path
        if not file.is_file():
            errors.append(f"missing agent role: {path}")
            continue
        content = file.read_text(encoding="utf-8")
        for term in REVIEW_TERMS[role]:
            if term.lower() not in content.lower():
                errors.append(f"{path} missing required boundary: {term}")
    for prompt in PROMPTS:
        path = f".pi/prompts/{prompt}.md"
        file = root / path
        if not file.is_file():
            errors.append(f"missing Pi prompt: {path}")
        elif "AGENTS.md" not in file.read_text(encoding="utf-8"):
            errors.append(f"{path} must reference AGENTS.md")
    return errors


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    errors = validate(root)
    for error in errors:
        print(f"FAIL: {error}")
    if not errors:
        print("PASS: Pi agentic role and policy contract")
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(main())
