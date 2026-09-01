//! `docs/BACKLOG.md` has to survive being forgotten. This is what fails when it
//! does not.
//!
//! On 2026-08-31 an edit that rewrote one section of that file by locating its
//! start and the start of the next one deleted an entire unrelated section
//! sitting between them, along with the process item added an hour earlier. It
//! was caught by chance, while counting sections for something else. Nothing
//! would have caught it otherwise: a docs file has no compiler, no test, and a
//! diff that looks plausible if you are reading the part you meant to change.
//!
//! That is worse for this file than for most, because its whole purpose is to
//! hold the things nobody is currently working on. A deletion from it removes
//! the only record that something was deferred, and the thing it recorded then
//! looks like something nobody ever thought about. Deferred work that is
//! silently forgotten is indistinguishable from work that was never noticed,
//! which is the failure this file exists to prevent.
//!
//! So the section list is declared here rather than inferred from the file.
//! Removing a section becomes a deliberate act with two edits and a diff that
//! shows both, and an edit whose blast radius exceeded its target fails the same
//! gate as a broken build.
//!
//! It lives in `tests/` rather than beside a module, unlike every other test in
//! this crate, because it asserts something about the repository and not about
//! the binary. `cargo build --release` does not compile this directory and the
//! Dockerfile never copies it, so the image build cannot be affected by it.
//! `include_str!` means a missing file fails to compile rather than passing
//! vacuously, which is the other half of the guarantee: an empty check that
//! finds nothing is how the poller mapping stayed wrong for weeks.

const BACKLOG: &str = include_str!("../../docs/BACKLOG.md");

/// Every `## ` section the backlog carries, in file order.
///
/// Update this when adding or removing a section, and only then. A section that
/// disappears without this list changing is a mistake by definition.
const SECTIONS: [&str; 10] = [
    "Deferred from AUD-013, retry logic",
    "Deferred from the container and edge work",
    "Deferred from the alerting work",
    "Deferred data correctness",
    "Operational",
    "Measurement",
    "Open audit findings",
    "Enumeration coverage",
    "Process",
    "History",
];

fn sections_in_file() -> Vec<&'static str> {
    BACKLOG
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(str::trim)
        .collect()
}

#[test]
fn the_backlog_carries_exactly_the_sections_declared_here() {
    let found = sections_in_file();

    let missing: Vec<&&str> = SECTIONS
        .iter()
        .filter(|s| !found.contains(&**s))
        .collect();
    let extra: Vec<&&str> = found
        .iter()
        .filter(|s| !SECTIONS.contains(&**s))
        .collect();

    assert!(
        missing.is_empty(),
        "docs/BACKLOG.md no longer has these sections: {missing:?}. \
         If the removal was deliberate, delete them from SECTIONS in the same \
         commit so the diff shows both halves. If it was not, something ate \
         them, which is what this test is for."
    );
    assert!(
        extra.is_empty(),
        "docs/BACKLOG.md has sections that are not declared: {extra:?}. \
         Add them to SECTIONS so the next accidental deletion fails here."
    );
    assert_eq!(
        found, SECTIONS,
        "the sections are all present but not in the declared order"
    );
}

/// A section whose items are gone but whose heading survives is the same loss
/// wearing a heading, and the check above would not see it.
#[test]
fn no_declared_section_is_empty() {
    let mut current: Option<&str> = None;
    let mut items = 0usize;
    let mut empty: Vec<&str> = Vec::new();

    for line in BACKLOG.lines().chain(std::iter::once("## ")) {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(previous) = current
                && items == 0
            {
                empty.push(previous);
            }
            current = Some(heading.trim());
            items = 0;
        } else if line.starts_with("- ") {
            items += 1;
        }
    }

    assert!(
        empty.is_empty(),
        "these backlog sections have a heading and no items: {empty:?}. \
         A section that empties out is either finished, in which case remove \
         the heading and its entry in SECTIONS deliberately, or it lost its \
         contents to an edit that reached further than it meant to."
    );
}

/// Every finding the backlog carries a bullet for.
///
/// The section list above catches a whole section vanishing. It does not catch
/// one bullet vanishing out of a section that still has others, and that is the
/// failure that actually happened twice in two days: an edit that replaced the
/// span between two anchors deleted an entry that had been written into that
/// span moments earlier. Both times in this file, both times invisible to the
/// checks that existed, both times found by counting things by hand for an
/// unrelated reason.
///
/// The findings already have stable identifiers, so declaring them costs one
/// line each and makes a disappearance a compile-level fact rather than
/// something noticed later. Removing a finding stays possible and becomes
/// deliberate: delete the bullet and delete it here, in one commit, with both
/// halves in the diff.
///
/// Only `AUD-0NN` entries are covered. The `No ID` bullets are not, because
/// they have nothing stable to key on, which is a real gap and is why new
/// findings worth tracking should get an identifier rather than a description.
const FINDINGS: [&str; 23] = [
    "AUD-009", "AUD-011", "AUD-012", "AUD-013", "AUD-014", "AUD-015",
    "AUD-016", "AUD-017", "AUD-018", "AUD-019", "AUD-020", "AUD-021",
    "AUD-022", "AUD-023", "AUD-024", "AUD-025", "AUD-026", "AUD-027",
    "AUD-028", "AUD-029", "AUD-030", "AUD-031", "AUD-033",
];

/// Identifiers of every finding bullet in the file, in order, duplicates kept.
///
/// A finding can legitimately appear twice: `AUD-013` has its deferred pieces in
/// its own section and its remainder under the open findings, and `AUD-026` and
/// `AUD-027` do the same. So this compares sets, not counts.
fn findings_in_file() -> Vec<&'static str> {
    BACKLOG
        .lines()
        .filter_map(|l| l.strip_prefix("- **"))
        .filter_map(|l| l.split("**").next())
        .filter(|id| id.starts_with("AUD-") && id.len() == 7)
        .collect()
}

#[test]
fn every_declared_finding_still_has_a_bullet() {
    let found = findings_in_file();

    let missing: Vec<&&str> = FINDINGS.iter().filter(|id| !found.contains(&**id)).collect();
    let extra: Vec<&&str> = found
        .iter()
        .filter(|id| !FINDINGS.contains(&**id))
        .collect();

    assert!(
        missing.is_empty(),
        "docs/BACKLOG.md no longer has a bullet for {missing:?}.          If the finding was closed, remove it from FINDINGS in the same commit          so the diff shows both halves. If it was not, an edit ate it, which is          what this test is for: it has happened twice."
    );
    assert!(
        extra.is_empty(),
        "docs/BACKLOG.md has findings that are not declared: {extra:?}.          Add them to FINDINGS so the next accidental deletion fails here."
    );
}
