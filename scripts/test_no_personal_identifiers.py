"""The repository ships to other organisations, so no tracked file may name a person or a personal account, or point
at one person's resources: GitHub handles, personal e-mail addresses, or local working-notes paths
and files. Repository links use `<org>` where the reader fills in their own; a literal one in the tree is a leak.

The forbidden values are assembled from parts, so this file does not itself contain them.

    python -m unittest discover -s scripts
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent

# A handle that once owned this repository. Parts only, never the whole literal.
_HANDLES = ("veera" + "rakesh" + "56",)
_FORBIDDEN = [
    ("a personal GitHub handle", re.compile("|".join(map(re.escape, _HANDLES)), re.IGNORECASE)),
    ("a local working-notes path", re.compile(re.escape("/tmp/" + "claude-"))),
    ("an internal notes file", re.compile(r"\b(?:" + "RULES" + r"|MERGE-" + "QUEUE" + r"|OWNER-[A-Z][A-Z-]*|" + "MEMORY"
                                          + r")\.md\b")),
    ("a personal e-mail address", re.compile(
        r"(?i)\b[\w.%+-]{1,64}@(?:gmail|googlemail|outlook|hotmail|live|msn|yahoo|ymail|icloud|me|mac|aol|proton|"
        r"protonmail|pm|gmx|zoho|yandex|rediffmail|qq|163|126)\.(?:com|net|org|me|in|co\.uk|de|fr|ch)\b")),
]


def _tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard"], cwd=ROOT,
                         capture_output=True, text=True, check=True).stdout
    return [line for line in out.splitlines() if line and (ROOT / line).is_file()]


def findings(paths: list[str], root: pathlib.Path = ROOT) -> list[str]:
    found = []
    for rel in paths:
        text = (root / rel).read_bytes().decode("utf-8", "replace")
        found += [f"{rel}: {what}" for what, rx in _FORBIDDEN if rx.search(text)]
    return found


class NoPersonalIdentifiers(unittest.TestCase):
    def test_no_tracked_file_names_a_person_or_a_personal_resource(self):
        try:
            paths = _tracked()
        except (OSError, subprocess.CalledProcessError):
            self.skipTest("not a git checkout")
        self.assertEqual(findings(paths), [])

    def test_each_kind_of_identifier_is_caught(self):
        planted = [
            (_HANDLES[0].upper() + "/helios", "a personal GitHub handle"),
            ("see " + "/tmp/" + "claude-0" + "/notes", "a local working-notes path"),
            ("per " + "MERGE-" + "QUEUE" + ".md", "an internal notes file"),
            ("mail " + "someone" + "@" + "gmail" + ".com", "a personal e-mail address"),
        ]
        for text, what in planted:
            with self.subTest(what=what), tempfile.TemporaryDirectory() as d:
                (pathlib.Path(d) / "f.txt").write_text(f"x {text} y\n", encoding="utf-8")
                self.assertEqual(findings(["f.txt"], pathlib.Path(d)), [f"f.txt: {what}"])

    def test_placeholders_are_not_findings(self):
        with tempfile.TemporaryDirectory() as d:
            (pathlib.Path(d) / "README.md").write_text(
                "uses: <org>/helios/action@<commit-sha>\nhttps://github.com/<org>/helios\n", encoding="utf-8")
            self.assertEqual(findings(["README.md"], pathlib.Path(d)), [])


if __name__ == "__main__":
    unittest.main()
