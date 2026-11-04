import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


WORKFLOW = Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml"
STEP_NAME = "Configure persistent cargo target dir"


def extract_run_script(text, step_name):
    """Return the shell body of a workflow step's `run: |` block.

    Fails loudly instead of raising IndexError when the step is renamed,
    re-indented, or switches away from a block scalar.
    """
    marker = re.search(rf"^ *- name: {re.escape(step_name)} *$", text, re.MULTILINE)
    if marker is None:
        raise AssertionError(f"workflow step not found: {step_name!r}")
    tail = text[marker.end():]
    run_match = re.search(r"^( *)run: \|\n", tail, re.MULTILINE)
    if run_match is None:
        raise AssertionError(f"step {step_name!r} has no 'run: |' block")
    body_indent = len(run_match.group(1)) + 2
    lines = []
    for line in tail[run_match.end():].splitlines():
        if line.strip() and len(line) - len(line.lstrip()) < body_indent:
            break
        lines.append(line[body_indent:] if line.strip() else "")
    if not lines:
        raise AssertionError(f"step {step_name!r} has an empty 'run: |' block")
    return "\n".join(lines) + "\n"


class PersistentTargetTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.script = extract_run_script(WORKFLOW.read_text(), STEP_NAME)

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
        self.assertIn("RUNNER_NAME is unset", result.stderr)
        self.assertEqual(assignment, "")
        self.assertEqual(list(home.iterdir()), [])

    def test_unsafe_runner_name_is_rejected(self):
        home = self.root / "home"
        home.mkdir()
        for unsafe in ("../escape", "a/b", ".."):
            with self.subTest(runner=unsafe):
                result, assignment = self.run_step(home, unsafe)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("refusing unsafe RUNNER_NAME", result.stderr)
                self.assertEqual(assignment, "")
        # Nothing was created outside the intended target location.
        self.assertEqual(sorted(p.name for p in home.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
