import tempfile
import unittest
from pathlib import Path
from check_sdd_contract import validate, REQUIREMENTS, DESIGN, MATRIX, SPEC, GOVERNANCE


class SddContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        fixtures = {
            REQUIREMENTS: "- **M31-R1** One.\n- **M31-N1** Two.\nmaintainer agreement required\nopenspec/specs/m31-tokio-supervisor.md",
            DESIGN: "M31-R1 M31-N1\napproval pending\nopenspec/specs/m31-tokio-supervisor.md",
            MATRIX: "| M31-R1 | R1 | test | pending | Partial |\n| M31-N1 | R2 | test | pending | Partial |",
            SPEC: "m31-worker-supervision.md",
            GOVERNANCE: "Draft -> approved",
        }
        for name, content in fixtures.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_valid_baseline(self):
        self.assertEqual(validate(self.root), [])

    def test_missing_design_reference(self):
        path = self.root / DESIGN
        path.write_text("M31-R1\napproval pending\nopenspec/specs/m31-tokio-supervisor.md")
        self.assertTrue(any("missing M31-N1" in error for error in validate(self.root)))

    def test_missing_acceptance_row(self):
        (self.root / MATRIX).write_text("| M31-R1 | R1 | test | pending | Partial |")
        self.assertTrue(any("missing M31-N1" in error for error in validate(self.root)))

    def test_undefined_requirement(self):
        with (self.root / MATRIX).open("a") as handle:
            handle.write("\n| M31-R99 | R99 | test | pending | Partial |")
        self.assertTrue(any("undefined M31-R99" in error for error in validate(self.root)))

    def test_missing_artifact(self):
        (self.root / DESIGN).unlink()
        self.assertTrue(any("missing SDD artifact" in error for error in validate(self.root)))


if __name__ == "__main__":
    unittest.main()
