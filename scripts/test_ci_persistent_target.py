import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


WORKFLOW = Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml"


class PersistentTargetTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        step = WORKFLOW.read_text().split("      - name: Configure persistent cargo target dir\n", 1)[1]
        self.script = textwrap.dedent(step.split("        run: |\n", 1)[1].split("\n      - ", 1)[0])

    def run_step(self, home, runner=None):
        envfile = self.root / (home.name + ".env")
        envfile.touch()
        env = os.environ.copy()
        env.update(HOME=str(home), GITHUB_ENV=str(envfile))
        env.pop("RUNNER_NAME", None)
        if runner is not None:
            env["RUNNER_NAME"] = runner
        result = subprocess.run(["bash", "-c", self.script], env=env, capture_output=True, text=True)
        return result, envfile.read_text()

    def test_distinct_runner_homes_and_reuse_without_copy(self):
        first, second = self.root / "home1", self.root / "home2"
        first.mkdir()
        second.mkdir()
        seed = first / "kcode-target"
        seed.mkdir()
        (seed / "seed-marker").write_text("seed")
        result, assignment = self.run_step(first, "desk-kcode-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        target1 = first / "kcode-target-desk-kcode-1"
        self.assertEqual(assignment, f"CARGO_TARGET_DIR={target1}\n")
        self.assertTrue(target1.is_dir())
        self.assertFalse((target1 / "seed-marker").exists())
        marker = target1 / "existing"
        marker.write_text("preserved")
        result, _ = self.run_step(first, "desk-kcode-1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(marker.read_text(), "preserved")
        result, assignment = self.run_step(second, "desk-kcode-2")
        self.assertEqual(result.returncode, 0, result.stderr)
        target2 = second / "kcode-target-desk-kcode-2"
        self.assertEqual(assignment, f"CARGO_TARGET_DIR={target2}\n")
        self.assertTrue(target2.is_dir())
        self.assertFalse((target2 / "existing").exists())

    def test_missing_runner_name_fails_without_export(self):
        home = self.root / "home"
        home.mkdir()
        result, assignment = self.run_step(home)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(assignment, "")
        self.assertEqual(list(home.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
