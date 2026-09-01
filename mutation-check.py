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
        text = original.decode("utf-8")
        for old, new in spec["edits"]:
            if old not in text:
                return {
                    "name": spec["name"],
                    "error": f"anchor not found in {spec['file']}: {old[:120]!r}",
                }
            text = text.replace(old, new, 1)
        path.write_bytes(text.encode("utf-8"))

        proc = subprocess.run(
            spec["check"], cwd=root, shell=True, capture_output=True, text=True
        )
        output = proc.stdout + proc.stderr
        return {
            "name": spec["name"],
            "file": spec["file"],
            "failed": failing_lines(output),
            "clean": not any(m in output for m in FAIL_MARKERS),
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
        return 0 if ok else 1
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
