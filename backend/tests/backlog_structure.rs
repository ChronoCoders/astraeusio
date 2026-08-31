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
