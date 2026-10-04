"""Fixture tests for advisory-check.py.

`advisories.json` is empty, so running the checker against the live tree proves
nothing: an empty decision file and a parser that finds no advisories produce
the same output. These drive it from a captured `cargo audit --json` carrying
one unsound and one yanked advisory, and assert each of the four behaviours.
"""
import importlib.util
import io
import json
import pathlib
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

ROOT = pathlib.Path(__file__).resolve().parent

_spec = importlib.util.spec_from_file_location("advisory_check", ROOT / "advisory-check.py")
ac = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ac)


def report(*advisories):
    """A cargo audit report in the shape the real one has."""
    warnings = {}
    for adv_id, crate, version, kind in advisories:
        warnings.setdefault(kind, []).append(
            {
                "kind": kind,
                "package": {"name": crate, "version": version},
                "advisory": {"id": adv_id},
            }
        )
    return {"vulnerabilities": {"count": 0, "list": []}, "warnings": warnings}


UNSOUND = ("RUSTSEC-2026-0190", "anyhow", "1.0.102", "unsound")
YANKED = ("RUSTSEC-0000-0000", "spin", "0.9.8", "yanked")


def run(rep, decisions):
    buf = io.StringIO()
    with redirect_stdout(buf):
        failures = ac.run(rep, decisions)
    return failures, buf.getvalue()


class TheFixtureItself(unittest.TestCase):
    """Without this the assertions below could all pass on an empty report."""

    def test_the_fixture_carries_both_advisories(self):
        found = ac.informational(report(UNSOUND, YANKED))
        self.assertEqual([f["id"] for f in found], ["RUSTSEC-2026-0190", "RUSTSEC-0000-0000"])
        self.assertEqual({f["kind"] for f in found}, {"unsound", "yanked"})

    def test_a_real_audit_report_parses_to_nothing_today(self):
        """The live shape, with warnings as an empty dict, must not crash."""
        self.assertEqual(ac.informational({"warnings": {}, "vulnerabilities": {"count": 0}}), [])


class AnUndeclaredAdvisory(unittest.TestCase):
    def test_it_fails_and_says_so_by_name(self):
        failures, out = run(report(UNSOUND), {"informational": []})
        self.assertEqual(failures, 1)
        self.assertIn("UNDECLARED", out)
        self.assertIn("RUSTSEC-2026-0190", out)
        self.assertIn("anyhow", out, "the crate must be named, not just the id")


class ADeclaredAdvisoryWhoseCheckHolds(unittest.TestCase):
    def test_symbol_absent_lets_it_through_and_names_what_it_establishes(self):
        decisions = {
            "informational": [
                {
                    "id": "RUSTSEC-2026-0190",
                    "crate": "anyhow",
                    "check": "symbol-absent",
                    "symbol": "a_symbol_no_rust_file_here_contains",
                    "path": "backend/src",
                }
            ]
        }
        failures, out = run(report(UNSOUND), decisions)
        self.assertEqual(failures, 0, out)
        self.assertIn("reachable=no", out)
        self.assertIn("call-site absence", out, "the entry must say what it establishes")
        self.assertIn("control:", out, "the floor must appear in the output")

    def test_crate_absent_establishes_the_stronger_claim(self):
        decisions = {
            "informational": [
                {
                    "id": "RUSTSEC-0000-0000",
                    "crate": "spin",
                    "check": "crate-absent",
                    "crate_name": "spin",
                }
            ]
        }
        # `crate` doubles as the parameter, so this resolves against the real tree.
        decisions["informational"][0]["crate"] = "a-crate-that-is-not-a-dependency"
        failures, out = run(report(YANKED), decisions)
        self.assertEqual(failures, 0, out)
        self.assertIn("dependency reachability", out)


class ADeclaredAdvisoryWhoseCheckFails(unittest.TestCase):
    def test_a_symbol_that_is_present_blocks(self):
        decisions = {
            "informational": [
                {
                    "id": "RUSTSEC-2026-0190",
                    "crate": "anyhow",
                    "check": "symbol-absent",
                    # Every Rust file in the crate has this, so the claim is false.
                    "symbol": "fn ",
                    "path": "backend/src",
                }
            ]
        }
        failures, out = run(report(UNSOUND), decisions)
        self.assertEqual(failures, 1)
        self.assertIn("REACHABLE", out)
        self.assertIn("no longer holds", out)

    def test_an_unknown_check_type_fails_rather_than_passing(self):
        decisions = {
            "informational": [
                {"id": "RUSTSEC-2026-0190", "crate": "anyhow", "check": "trust-me"}
            ]
        }
        failures, out = run(report(UNSOUND), decisions)
        self.assertEqual(failures, 1)
        self.assertIn("BAD CHECK", out)

    def test_a_path_with_no_rust_files_fails_rather_than_reading_as_absence(self):
        """The floor, exercised separately from the missing-path guard.

        A path that exists but holds no source is the case where a scan
        returns nothing and that nothing means nothing. An earlier draft of
        the mutation set pointed at a missing path instead, which a different
        guard catches, so this floor went untested.
        """
        decisions = {
            "informational": [
                {
                    "id": "RUSTSEC-2026-0190",
                    "crate": "anyhow",
                    "check": "symbol-absent",
                    "symbol": "downcast_mut",
                    "path": "docs",
                }
            ]
        }
        failures, out = run(report(UNSOUND), decisions)
        self.assertEqual(failures, 1, out)
        self.assertIn("proves nothing", out)

    def test_a_path_that_does_not_exist_fails(self):
        decisions = {
            "informational": [
                {
                    "id": "RUSTSEC-2026-0190",
                    "crate": "anyhow",
                    "check": "symbol-absent",
                    "symbol": "downcast_mut",
                    "path": "backend/not-a-directory",
                }
            ]
        }
        failures, out = run(report(UNSOUND), decisions)
        self.assertEqual(failures, 1, "a scan that read nothing must not read as absence")


class AnEntryForSomethingNoLongerReported(unittest.TestCase):
    def test_it_is_listed_as_stale_and_does_not_fail(self):
        decisions = {
            "informational": [
                {
                    "id": "RUSTSEC-1999-0001",
                    "crate": "gone",
                    "check": "symbol-absent",
                    "symbol": "x",
                    "path": "backend/src",
                }
            ]
        }
        failures, out = run(report(), decisions)
        self.assertEqual(failures, 0, "a stale entry is rot, not a fault")
        self.assertIn("stale", out)
        self.assertIn("RUSTSEC-1999-0001", out)


class TheShippedDecisionFile(unittest.TestCase):
    def test_it_parses_and_every_entry_uses_a_known_check(self):
        d = json.loads((ROOT / "advisories.json").read_text(encoding="utf-8"))
        for e in d["informational"]:
            self.assertIn(
                e.get("check"), ac.KNOWN_CHECKS,
                "%s uses an unknown check type" % e.get("id"),
            )
            self.assertNotIn(
                "predicate", e,
                "entries carry parameters, never a command for the gate to run",
            )


if __name__ == "__main__":
    unittest.main()
