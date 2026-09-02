#!/usr/bin/env python3
"""Reintroduce a defect, run a check, restore the file. Without losing work.

A test that passes when its defect is put back guards nothing, so proving one
bites means editing the source, running the suite, and putting the source back.
The putting back is where this went wrong twice in one day, both times the same
way: the restore was `git checkout -- <file>`, which restores the file to HEAD
and therefore throws away every uncommitted change in it, including the test
that had just been written and not yet committed.

The rule "commit before mutating" is not a fix, because it is a thing to
remember at exactly the moment attention is on the mutation. So this restores
from a byte snapshot taken before the edit, never from git. The snapshot does
not know or care what is committed, which is what makes losing uncommitted work
impossible here rather than unlikely:

  - the file's exact bytes are read before anything is written, kept in memory
    and also written to a sibling `.mutation-backup` file, so a hard kill still
    leaves a recoverable copy on disk
  - the edit, the check and the restore sit in try/finally, so a crash, an
    exception or a failing check all still restore
  - after restoring, the bytes are compared against the snapshot and the run
    fails loudly if they differ
  - git is never invoked, so there is nothing that can reach past the snapshot

Two later corrections, both in the direction that made the harness lie rather
than complain. Anchors are written with plain newlines and this repository's
Rust is CRLF, so any anchor spanning a line break was declined as "anchor not
found", which reads as the caller's mistake. And a check that failed without
printing a recognised marker was reported clean, so a live defect came back as
"NOTHING FAILED". Both are cases in --self-test now.

Usage:

    python mutation-check.py mutations.json
    python mutation-check.py --self-test

The spec is a list of objects:

    [
      {
        "name": "removing the audience check",
        "file": "backend/src/auth.rs",
        "edits": [["validation_for(AUD_SESSION)", "Validation::default()"]],
        "check": "cargo test --manifest-path backend/Cargo.toml"
      }
    ]

Each `edits` pair is an exact string replacement, applied once, and an anchor
that is not found aborts that mutation rather than guessing. The check's output
is scanned for the usual failure shapes, so it works for `cargo test`,
`unittest` and `eslint` without being told which is which.
"""

import json
import subprocess
import sys
from pathlib import Path

CRLF = chr(13) + chr(10)
LF = chr(10)

FAIL_MARKERS = ("FAILED", "FAIL:", "ERROR:", "error[", "error:", "✖")


def failing_lines(output):
    """Test names and errors, whichever runner produced them."""
    out = []
    for line in output.splitlines():
        stripped = line.strip()
        if stripped.startswith("test ") and "FAILED" in stripped:
            out.append(stripped.split("...")[0].removeprefix("test ").strip())
        elif stripped.startswith(("FAIL: ", "ERROR: ")):
            out.append(stripped.split(" ", 1)[1].split(" ")[0])
        elif stripped.startswith(("error[", "error: ")):
            out.append(stripped[:100])
    return out


def run_one(spec, root):
    path = root / spec["file"]
    if not path.is_file():
        return {"name": spec["name"], "error": f"no such file: {spec['file']}"}

    original = path.read_bytes()
    backup = path.with_suffix(path.suffix + ".mutation-backup")
    backup.write_bytes(original)

    try:
        # Anchors are written with plain newlines, and most of this repository's
        # Rust is CRLF. Matching raw bytes meant every anchor spanning a line
        # break reported "anchor not found", which reads as a bad spec rather
        # than as the tool declining to look. So match on a normalised view and
        # put the file's own ending back before writing, leaving endings alone.
        text = original.decode("utf-8")
        crlf = CRLF in text
        if crlf:
            text = text.replace(CRLF, LF)
        for old, new in spec["edits"]:
            if old not in text:
                return {
                    "name": spec["name"],
                    "error": f"anchor not found in {spec['file']}: {old[:120]!r}",
                }
            text = text.replace(old, new, 1)
        if crlf:
            text = text.replace(LF, CRLF)
        path.write_bytes(text.encode("utf-8"))

        proc = subprocess.run(
            spec["check"], cwd=root, shell=True, capture_output=True, text=True
        )
        output = proc.stdout + proc.stderr
        # The exit code counts as much as the text. A check that fails quietly,
        # which is what this file's own self test does when it reports NO and
        # returns 1, matched none of the markers and was reported as
        # "NOTHING FAILED": the harness claimed a real defect was unguarded.
        # That is the false negative direction, and it is the one that makes a
        # mutation check worthless rather than merely noisy.
        failed = failing_lines(output)
        marked = any(m in output for m in FAIL_MARKERS)
        if proc.returncode != 0 and not failed:
            failed = [f"the check exited {proc.returncode} without naming a test"]
        return {
            "name": spec["name"],
            "file": spec["file"],
            "failed": failed,
            "clean": proc.returncode == 0 and not marked,
        }
    finally:
        # Unconditional, and from the snapshot rather than from git, so an
        # exception above cannot leave the tree mutated or the file rolled back
        # to whatever HEAD happens to hold.
        path.write_bytes(original)
        if path.read_bytes() != original:
            raise SystemExit(
                f"RESTORE FAILED for {spec['file']}. The bytes before the edit are "
                f"in {backup}. Do not run anything else until that is sorted out."
            )
        backup.unlink(missing_ok=True)


def self_test(root):
    """Prove the restore survives a check that throws, on a file git has never
    seen, which is the case `git checkout` cannot handle at all."""
    probe = root / ".mutation-selftest.tmp"
    content = b"uncommitted work that no commit contains\n"
    probe.write_bytes(content)
    try:
        try:
            run_one(
                {
                    "name": "self test",
                    "file": probe.name,
                    "edits": [["uncommitted", "MUTATED"]],
                    "check": "python -c \"raise SystemExit(1)\"",
                },
                root,
            )
        except SystemExit:
            raise
        after = probe.read_bytes()
        ok = after == content
        print(f"  restored after a failing check: {'yes' if ok else 'NO'}")
        print(f"  file was never committed:       yes")
        print(f"  backup cleaned up:              "
              f"{'yes' if not (root / '.mutation-selftest.tmp.mutation-backup').exists() else 'NO'}")
        crlf_ok = _crlf_case(root)
        print(f"  CRLF anchor spanning a newline: {'yes' if crlf_ok else 'NO'}")
        quiet_ok = _quiet_failure_case(root)
        print(f"  a check that fails silently:    {'yes' if quiet_ok else 'NO'}")
        return 0 if (ok and crlf_ok and quiet_ok) else 1
    finally:
        probe.unlink(missing_ok=True)


def _crlf_case(root):
    """Anchors are written with plain newlines. Every Rust file in this
    repository is CRLF, so matching raw bytes declined every anchor that
    spanned a line break and said "anchor not found", which looks like a bad
    spec. Kept as a case because the failure was silent in the direction that
    reads as the caller's fault."""
    probe = root / ".mutation-crlf.tmp"
    snap = root / ".mutation-crlf.snap"
    before = CRLF.join(["one", "two", "three"]).encode("utf-8")
    probe.write_bytes(before)
    # The check copies the file while it is mutated, because the restore has
    # already run by the time run_one returns. Paths go through argv rather
    # than into the -c source, so nothing here depends on shell quoting.
    copy = 'python -c "import shutil,sys;shutil.copyfile(sys.argv[1],sys.argv[2])"'
    try:
        result = run_one(
            {
                "name": "crlf",
                "file": probe.name,
                "edits": [[LF.join(["one", "two"]), LF.join(["one", "CHANGED"])]],
                "check": f'{copy} "{probe}" "{snap}"',
            },
            root,
        )
        if "error" in result or not snap.exists():
            return False
        during = snap.read_bytes()
        # The edit landed, the endings were put back, and the file came home.
        return (
            during.count(b"CHANGED") == 1
            and during.count(CRLF.encode()) == 2
            and probe.read_bytes() == before
        )
    finally:
        probe.unlink(missing_ok=True)
        snap.unlink(missing_ok=True)


def _quiet_failure_case(root):
    """A check can fail without printing anything a marker matches. Scanning
    output alone reported those as clean, so the harness said a live defect was
    unguarded, which is the one wrong answer it must never give."""
    probe = root / ".mutation-quiet.tmp"
    probe.write_bytes(b"anchor" + LF.encode())
    try:
        result = run_one(
            {
                "name": "quiet",
                "file": probe.name,
                "edits": [["anchor", "mutated"]],
                "check": "python -c \"raise SystemExit(1)\"",
            },
            root,
        )
        return "error" not in result and not result["clean"] and bool(result["failed"])
    finally:
        probe.unlink(missing_ok=True)


def main():
    root = Path(__file__).resolve().parent
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        print("mutation-check self test")
        raise SystemExit(self_test(root))
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)

    specs = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    worst = 0
    for spec in specs:
        result = run_one(spec, root)
        print(f"\n### {result['name']}")
        if "error" in result:
            print(f"  {result['error']}")
            worst = 1
            continue
        if result["failed"]:
            print(f"  file: {result['file']}")
            print(f"  the defect is caught by {len(result['failed'])}:")
            for name in result["failed"]:
                print(f"    {name}")
        else:
            print(f"  file: {result['file']}")
            print("  NOTHING FAILED. Whatever this defect breaks, no check sees it.")
            worst = 1
    raise SystemExit(worst)


if __name__ == "__main__":
    main()
