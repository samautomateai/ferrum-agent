import tempfile
import unittest
from pathlib import Path

from check_agentic_contract import PROMPTS, REQUIRED, ROLES, validate


class AgenticContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        source = Path(__file__).resolve().parents[1]
        for path in (*REQUIRED, *(f"docs/agentic/roles/{role}.md" for role in ROLES), *(f".pi/prompts/{prompt}.md" for prompt in PROMPTS)):
            dest = self.root / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text((source / path).read_text(encoding="utf-8"), encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_valid_contract(self):
        self.assertEqual(validate(self.root), [])

    def test_missing_pi_prompt_fails(self):
        (self.root / ".pi/prompts/review.md").unlink()
        self.assertTrue(any("missing Pi prompt" in e for e in validate(self.root)))

    def test_missing_role_fails(self):
        (self.root / "docs/agentic/roles/security.md").unlink()
        self.assertTrue(any("missing agent role" in e for e in validate(self.root)))

    def test_missing_policy_fails(self):
        (self.root / "AGENTS.md").write_text("no policy", encoding="utf-8")
        self.assertTrue(any("missing policy" in e for e in validate(self.root)))

    def test_review_boundary_fails(self):
        (self.root / "docs/agentic/roles/reviewer.md").write_text("Review", encoding="utf-8")
        self.assertTrue(any("reviewer.md" in e for e in validate(self.root)))


if __name__ == "__main__":
    unittest.main()
