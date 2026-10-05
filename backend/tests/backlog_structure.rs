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
const SECTIONS: [&str; 11] = [
    "Deferred from AUD-013, retry logic",
    "Deferred from the container and edge work",
    "Deferred from the alerting work",
    "Deferred data correctness",
    "Operational",
    "Measurement",
    "Who the users are",
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

    let missing: Vec<&&str> = SECTIONS.iter().filter(|s| !found.contains(&**s)).collect();
    let extra: Vec<&&str> = found.iter().filter(|s| !SECTIONS.contains(&**s)).collect();

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
const FINDINGS: [&str; 35] = [
    "AUD-009", "AUD-011", "AUD-012", "AUD-013", "AUD-014", "AUD-015",
    // Closed as a vulnerability; the bullet that remains is the deferred PKCE
    // half, so the identifier stays declared while that text does.
    "AUD-020", "AUD-022", "AUD-026", "AUD-027", "AUD-030", "AUD-031", "AUD-033",
    // Both found 2026-09-22 by looking for constants with the shape of the
    // backup size floor: a value that was right until another change moved
    // what it measures.
    "AUD-034", "AUD-035",
    // A test fixture that only fails in the first forty minutes of a UTC
    // day. Declared late: it was reported as recorded twice before it was
    // written, and a finding missing from both the bullets and this list is
    // consistent in both directions, so the structure test could not see it.
    "AUD-036", "AUD-037",
    // Said in a report and written nowhere until 2026-09-23. A finding
    // absent from both the bullets and this list is consistent in both
    // directions, so nothing here could have caught their absence.
    "AUD-038", "AUD-039", "AUD-040", "AUD-041", "AUD-042",
    // The sleep-after-work period, and the 25 hour outage found while measuring
    // it. AUD-043 was recorded with an assumed fetch time and corrected the same
    // day against the period the logs already held.
    "AUD-043", "AUD-044",
    // The 2026-09-23 audit, re-verified against 0947fd0 and recorded 2026-10-04.
    // Eleven days in conversation only, which is how AUD-038 to AUD-041 were also
    // called recorded while absent from this list and from the bullets.
    "AUD-045", "AUD-046", "AUD-047", "AUD-048", "AUD-049", "AUD-050", "AUD-051",
    // Both found by hunting the AUD-049 shape through Store::open rather than
    // from any report. Neither is the same fail-open class, and the entries say
    // which direction each one fails in, which is the part that decides.
    "AUD-052", "AUD-053",
    // A method finding rather than a code one: five checks in one session that
    // compared two artifacts and could not see both being wrong the same way.
    // Declared because the comment above this list says a finding worth tracking
    // gets an identifier, and because the fifth instance hid a live defect.
    "AUD-054",
    // The hooks enforce the naming rule and nothing else, so four commit message
    // rules this project follows are habits rather than gates. Read from the
    // hook files, not from the notes that describe them.
    "AUD-055",
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

    let missing: Vec<&&str> = FINDINGS
        .iter()
        .filter(|id| !found.contains(&**id))
        .collect();
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

/// Findings that are fully closed, and the date their marker carries.
///
/// One accepted form, and the identifier is inside the marker:
///
/// ```text
/// **AUD-NNN closed YYYY-MM-DD**
/// ```
///
/// The identifier is in the marker because a marker without one cannot be
/// attributed by machine. `AUD-012`'s closure sits seventy three lines below its
/// own bullet with other top level list items in between, so slicing a bullet and
/// reading the closure inside it does not work. Four spellings were in use until
/// 2026-10-05 and counting them by eye was wrong three times in one session: a
/// pattern that matched only the compact spelling saw three of seven, reading the
/// first `Closed` line in a bullet read a plan sentence as a closure, and a
/// `No ID` bullet's closure was attributed to the finding above it.
///
/// A partial closure must not use this form. `AUD-027` shipped one half and kept
/// the other open, and it says that in words instead.
const CLOSED: [(&str, &str); 9] = [
    // Closed the other way: the claim came off rather than the delay going in.
    ("AUD-012", "2026-09-22"),
    ("AUD-036", "2026-10-04"),
    // The 2026-09-23 audit, closed 2026-10-04 and 2026-10-05.
    ("AUD-045", "2026-10-05"),
    ("AUD-046", "2026-10-05"),
    // Both were fixed in code on 2026-10-04 and left unmarked here for a day,
    // which is how a tally of this file came out two short.
    ("AUD-047", "2026-10-04"),
    ("AUD-048", "2026-10-05"),
    ("AUD-049", "2026-10-04"),
    ("AUD-050", "2026-10-05"),
    ("AUD-051", "2026-10-05"),
];

/// Every closure marker in the file, as (identifier, date).
///
/// Parsed by hand rather than with a pattern crate, because this directory has no
/// dependencies and should not gain one for nine lines of string work.
fn closure_markers() -> Vec<(&'static str, &'static str)> {
    const OPEN: &str = "**AUD-";
    const MID: &str = " closed ";
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(hit) = BACKLOG[from..].find(OPEN) {
        let at = from + hit + 2;
        from = at + 4;
        let tail = &BACKLOG[at..];
        if tail.len() < 27 {
            continue;
        }
        let (id, rest) = tail.split_at(7);
        if !id.starts_with("AUD-") || !id[4..].bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Some(rest) = rest.strip_prefix(MID) else {
            continue;
        };
        if rest.len() < 12 {
            continue;
        }
        let (date, close) = rest.split_at(10);
        let shaped = date.len() == 10
            && date.as_bytes()[4] == b'-'
            && date.as_bytes()[7] == b'-'
            && date
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
        if shaped && close.starts_with("**") {
            out.push((id, date));
        }
    }
    out
}

/// A closure is declared here and written in the one form, in both directions.
///
/// Without this the file carried four spellings and the only way to count what
/// was closed was to read it, which produced three different wrong answers in a
/// day. The declaration also keeps a closed finding from quietly losing its
/// bullet, since every identifier here must still be declared in `FINDINGS`.
#[test]
fn every_closure_is_declared_and_written_in_the_one_form() {
    let found = closure_markers();

    // A floor. If the parser above stops matching, `found` goes empty and the
    // comparisons below would all pass for a file with no closures at all.
    assert!(
        found.len() >= 5,
        "found {} closure markers, which is too few for this file to be right: \
         the parser is broken rather than the backlog",
        found.len()
    );

    let missing: Vec<&(&str, &str)> = CLOSED.iter().filter(|pair| !found.contains(pair)).collect();
    assert!(
        missing.is_empty(),
        "declared closed but not marked that way in docs/BACKLOG.md: {missing:?}. \
         The form is `**AUD-NNN closed YYYY-MM-DD**`, identifier included, and the \
         date here must match the one in the file."
    );

    let extra: Vec<&(&str, &str)> = found.iter().filter(|pair| !CLOSED.contains(pair)).collect();
    assert!(
        extra.is_empty(),
        "marked closed in docs/BACKLOG.md but not declared here: {extra:?}. \
         Add it to CLOSED in the same commit so the diff shows both halves."
    );

    let mut seen = found.clone();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(
        before,
        seen.len(),
        "a closure marker appears more than once, so one finding would answer for \
         two: {found:?}"
    );

    let undeclared: Vec<&&str> = CLOSED
        .iter()
        .map(|(id, _)| id)
        .filter(|id| !FINDINGS.contains(&**id))
        .collect();
    assert!(
        undeclared.is_empty(),
        "closed findings missing from FINDINGS: {undeclared:?}. A closed finding \
         still has a bullet, so it stays declared."
    );
}
