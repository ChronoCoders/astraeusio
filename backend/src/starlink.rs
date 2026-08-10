use reqwest::Client;
use thiserror::Error;

use crate::fetch::Fetched;

#[derive(Error, Debug)]
pub enum StarlinkError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Debug)]
pub struct StarlinkSat {
    pub norad_id: i32,
    pub name: String,
    pub tle_line1: String,
    pub tle_line2: String,
}

/// Fetches 3-line TLE text format and parses each name/line1/line2 triplet.
/// FORMAT=json from Celestrak returns GP elements without TLE strings;
/// FORMAT=tle returns the classic format that contains the actual TLE lines.
///
/// Celestrak refreshes every 2 hours and we poll hourly, so roughly every other
/// poll it answers 403 with "GP data has not updated since your last successful
/// download" (or 304 when it uses the conditional path). That is a no-change
/// response, not a failure and not an empty constellation, so it comes back as
/// [`PollOutcome::NoChange`] and the existing rows stand.
///
/// Nothing downstream skips on that. The poller still hands the empty batch to
/// the writer, and `Store::insert_starlink_batch` is what declines to touch the
/// table. That guard is load bearing: this table is a full replace, so an empty
/// batch reaching the DELETE would wipe it. An earlier version of this comment
/// claimed the poller skipped the insert, which was never true and made the
/// real protection look optional.
pub async fn fetch_starlink(client: &Client) -> Result<Fetched<StarlinkSat>, StarlinkError> {
    let resp = client
        .get("https://celestrak.org/NORAD/elements/gp.php?GROUP=starlink&FORMAT=tle")
        .send()
        .await?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED || status == reqwest::StatusCode::FORBIDDEN {
        return Ok(Fetched::no_change());
    }

    let text = resp.error_for_status()?.text().await?;

    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    // Three lines to a satellite. Counting the triplets upstream sent, rather
    // than the ones we managed to parse, is what separates "Celestrak sent an
    // empty file" from "the TLE format changed and we dropped every record".
    let received = lines.len() / 3;

    let mut sats = Vec::new();
    let mut i = 0;
    while i + 2 < lines.len() {
        let name = lines[i];
        let line1 = lines[i + 1];
        let line2 = lines[i + 2];

        // Validate TLE line identifiers before consuming the triplet.
        if !line1.starts_with('1') || !line2.starts_with('2') {
            i += 1;
            continue;
        }

        // NORAD catalog number occupies columns 3-7 (0-indexed: 2..7).
        if let Ok(norad_id) = line1[2..7].trim().parse::<i32>() {
            sats.push(StarlinkSat {
                norad_id,
                name: name.to_owned(),
                tle_line1: line1.to_owned(),
                tle_line2: line2.to_owned(),
            });
        }

        i += 3;
    }

    Ok(Fetched::lossy(sats, received))
}
