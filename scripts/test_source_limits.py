import tempfile
import unittest
from pathlib import Path
from check_source_limits import violations


class SourceLimitTests(unittest.TestCase):
    def test_valid_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'main.rs').write_text('fn main() {}\n')
            self.assertEqual(violations(root, 2, 20), [])

    def test_overlong_line_and_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'main.rs').write_text('123456\nabc\n')
            problems = violations(root, 1, 5)
            self.assertEqual(len(problems), 2)

    def test_excludes_vendor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'vendor').mkdir()
            (root / 'vendor' / 'generated.rs').write_text('x' * 100)
            self.assertEqual(violations(root, 1, 5), [])


if __name__ == '__main__':
    unittest.main()
