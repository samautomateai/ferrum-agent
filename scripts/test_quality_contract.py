import tempfile
import unittest
from pathlib import Path
from check_quality_contract import validate


class QualityContractTests(unittest.TestCase):
    def test_missing_taskfile(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIn('Taskfile.yml missing', validate(Path(d)))

    def test_missing_workflow(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'Taskfile.yml').write_text('version: 3')
            self.assertIn('quality.yml missing', validate(root))

    def test_detects_missing_lockfile(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / '.github/workflows').mkdir(parents=True)
            (root / 'Taskfile.yml').write_text('version: 3')
            (root / '.github/workflows/quality.yml').write_text('task quality')
            self.assertTrue(any('Cargo.lock' in x for x in validate(root)))


if __name__ == '__main__':
    unittest.main()
