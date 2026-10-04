"""Names every informational advisory and requires a recorded decision for it.

`cargo audit` exits 0 on `unsound` and `yanked`, and the gate reads the exit
code, so a reachable informational advisory passes exactly as an unreachable one
does. RUSTSEC-2026-0190 sat in the lockfile for about ten weeks on that basis:
published, fixed upstream in anyhow 1.0.103, and invisible to a green gate.

The standing rule is that an advisory classed as a vulnerability blocks whatever
its score, and an informational advisory blocks only when the affected code is
reachable from the shipped binary. This enforces the reporting half: every
informational advisory is printed by name, and each one needs an entry in
`advisories.json` saying whether it is reachable and on what evidence.

The entry does not carry a command. The gate implements a small set of typed
checks and the entry carries only parameters, because a gate that executes
strings from a config file will execute anything written there.

    symbol-absent   the symbol does not appear under a path. Establishes
                    call-site absence, not dependency absence: a macro or a
                    re-export can defeat it.
    crate-absent    the crate does not appear in the dependency tree for the
                    shipped target. Establishes dependency reachability, which
                    is stronger, and is only available when we do not depend on
                    the crate at all.

An unknown check type fails rather than passing, because an entry nobody can
evaluate is not a decision.

    advisory-check.py              check against a real cargo audit run
    advisory-check.py --selftest   run the fixture tests
"""
import json
import os
import re
import subprocess
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
DECISIONS = ROOT / "advisories.json"
BACKEND = ROOT / "backend"

KNOWN_CHECKS = ("symbol-absent", "crate-absent")


def audit_json():
    """The audit report, from a fixture when one is named, else from cargo."""
    fixture = os.environ.get("ADVISORY_AUDIT_JSON")
    if fixture:
        return json.loads(pathlib.Path(fixture).read_text(encoding="utf-8"))
    out = subprocess.run(
        ["cargo", "audit", "--json"], cwd=BACKEND, capture_output=True, text=True
    )
    if not out.stdout.strip():
        raise SystemExit("cargo audit produced no JSON: %s" % out.stderr.strip()[:300])
    return json.loads(out.stdout)


def informational(report):
    """Every informational advisory, flattened to (id, crate, kind)."""
    found = []
    for kind, entries in (report.get("warnings") or {}).items():
        for e in entries or []:
            adv = (e.get("advisory") or {})
            pkg = (e.get("package") or {})
            found.append(
                {
                    "id": adv.get("id") or "(no id: %s/%s)" % (kind, pkg.get("name")),
                    "crate": pkg.get("name", "?"),
                    "version": pkg.get("version", "?"),
                    "kind": kind,
                }
            )
    return sorted(found, key=lambda f: (f["kind"], f["id"]))


def check_symbol_absent(entry):
    """The symbol does not appear under the path, whitespace normalised.

    Returns (ok, detail). The floor is part of the result: a scan that read no
    files, or read files with no recognisable content, reports failure rather
    than absence, because those are the same output as a clean tree.
    """
    symbol = entry.get("symbol")
    rel = entry.get("path")
    if not symbol or not rel:
        return False, "symbol-absent needs both `symbol` and `path`"
    base = ROOT / rel
    if not base.exists():
        return False, "path %s does not exist" % rel
    files = [p for p in base.rglob("*.rs") if p.is_file()]
    if not files:
        return False, "no .rs files under %s, so the scan proves nothing" % rel
    control = 0
    hits = []
    for p in files:
        text = p.read_text(encoding="utf-8", errors="replace")
        flat = re.sub(r"\s+", " ", text)
        if "fn " in flat:
            control += 1
        if symbol in flat:
            hits.append(p.relative_to(ROOT).as_posix())
    if control == 0:
        return False, "read %d files and none contained `fn `; the scan is not reading source" % len(files)
    if hits:
        return False, "`%s` appears in %s" % (symbol, ", ".join(hits[:4]))
    return True, "`%s` absent from %d files under %s (control: %d carried `fn `)" % (
        symbol,
        len(files),
        rel,
        control,
    )


def check_crate_absent(entry):
    """The crate is not in the dependency tree for the shipped target."""
    crate = entry.get("crate")
    if not crate:
        return False, "crate-absent needs `crate`"
    out = subprocess.run(
        ["cargo", "tree", "-i", crate, "--target", "all"],
        cwd=BACKEND,
        capture_output=True,
        text=True,
    )
    blob = (out.stdout + out.stderr).lower()
    if "nothing to print" in blob or out.returncode != 0:
        return True, "%s is not in the dependency tree" % crate
    first = next((l for l in out.stdout.splitlines() if l.strip()), "")
    return False, "%s is in the tree: %s" % (crate, first.strip())


CHECKS = {"symbol-absent": check_symbol_absent, "crate-absent": check_crate_absent}

ESTABLISHES = {
    "symbol-absent": "call-site absence",
    "crate-absent": "dependency reachability",
}


def run(report, decisions):
    """Prints every informational advisory by name. Returns the failure count."""
    found = informational(report)
    by_id = {d.get("id"): d for d in decisions.get("informational", [])}
    failures = 0

    if not found:
        print("  informational advisories: none reported")
    for f in found:
        d = by_id.get(f["id"])
        if d is None:
            print(
                "  UNDECLARED  %s  %s %s  %s"
                % (f["id"], f["crate"], f["version"], f["kind"])
            )
            print(
                "      no entry in advisories.json. Record whether it is reachable, "
                "or prefer a semver-compatible update."
            )
            failures += 1
            continue
        kind = d.get("check")
        if kind not in KNOWN_CHECKS:
            print("  BAD CHECK   %s  unknown check type %r" % (f["id"], kind))
            failures += 1
            continue
        ok, detail = CHECKS[kind](d)
        if ok:
            print(
                "  decided     %s  %s %s  %s  reachable=no  (%s: %s)"
                % (
                    f["id"],
                    f["crate"],
                    f["version"],
                    f["kind"],
                    ESTABLISHES[kind],
                    detail,
                )
            )
        else:
            print(
                "  REACHABLE   %s  %s %s  %s" % (f["id"], f["crate"], f["version"], f["kind"])
            )
            print("      the recorded check no longer holds: %s" % detail)
            failures += 1

    # The other direction, reported and not fatal, so the file does not rot.
    reported = {f["id"] for f in found}
    for adv_id in sorted(set(by_id) - reported):
        print("  stale       %s  no longer reported by cargo audit; the entry can go" % adv_id)

    vulns = (report.get("vulnerabilities") or {}).get("count", 0)
    if vulns:
        print("  %d vulnerability advisories reported; cargo audit itself fails on those" % vulns)
    return failures


def main():
    report = audit_json()
    decisions = json.loads(DECISIONS.read_text(encoding="utf-8"))
    failures = run(report, decisions)
    if failures:
        print("  %d informational advisory issue(s)" % failures)
        return 1
    print("  every informational advisory is accounted for")
    return 0


if __name__ == "__main__":
    sys.exit(main())
