# Backlog

Work that was found, understood, and not done. One line per item.

Most of it was deferred on purpose and says why. The section on open audit findings is different:
those are simply still open, carried here so the tracked record does not depend on an untracked
report.

This exists because the deferrals were living in `docs/AUDIT-2026-08.md`, which is untracked and
therefore local to one machine. The audit report stays untracked until its findings are closed; this
file is the part that has to survive, so a decision to defer is not lost with a laptop.

Each line names its audit ID where it has one, what is deferred, and why. An item without an ID was
found outside the audit. Nothing here is a bug report: the detail lives in the audit under its ID,
or in the commit that created the deferral.

## Deferred from AUD-013, retry logic

- **AUD-013** `Retry-After` on a 429 is not honoured, because `error_for_status()` discards the
  response before the error reaches the retry layer; the shipped behaviour is stricter instead, so
  honouring the header is an improvement that touches every fetcher rather than a fix.
- **AUD-013** Webhook delivery still has no retry, despite `webhook_deliveries` carrying an attempt
  counter that only ever records 1, and it needs its own policy because the endpoint is the
  customer's and a failed delivery owes them something a poller retry does not.
- **AUD-013** Backoff does not scale with the poll interval: 250 ms then 500 ms covers a single bad
  request and nothing longer, and a constant cannot suit both a 60 s source and an hourly one, so
  the delay has to derive from the interval the way the per-attempt timeout already does.

## Deferred from the container and edge work

- **AUD-026** Stage 3, running the three containers as a non-root user, is deferred to a deploy of
  its own because it changes file ownership on the shared data volume and must not share a release
  with anything else.
- **AUD-027** The Content Security Policy ships as `Content-Security-Policy-Report-Only` and cannot
  move to enforcing until real browser reports show the policy does not break the app.
- **AUD-012** Authenticated Origin Pulls runs at `ssl_verify_client optional` and is not enforced,
  because 27 hours of observation saw exactly one Cloudflare colo (DFW) and roughly a third of that
  traffic is self-generated from the Dallas host, so the sample cannot establish that the fleet
  presents a valid certificate. The origin-side work is finished: as of `0e2fca0` no check depends
  on the site block serving a client that holds no certificate, and all four were shown to survive
  `ssl_verify_client on` against a throwaway nginx. What is missing is evidence about Cloudflare,
  not readiness here.
- No ID. SSH is exposed to the internet and takes roughly 1500 failed attempts a day with no
  fail2ban; key-only authentication is holding, so this is hardening rather than a live hole, and a
  proposal was never written.

## Deferred from the alerting work

- No ID. `healthcheck.sh` reports `status=000000` when the site is unreachable, because
  `curl -w "%{http_code}"` prints `000` and then exits non-zero so the `|| echo "000"` appends a
  second copy; harmless, since every spelling satisfies the `!= "200"` test, and deliberately not
  folded into a change about alert noise.

## Deferred data correctness

- No ID, described in the audit's section 9.4. The `> max_tag` pre-filter in the batch inserts means
  a record whose `time_tag` is older than the stored maximum can never be inserted, which makes the
  `ON CONFLICT DO UPDATE` clause written to absorb NOAA revisions unreachable for that purpose; how
  often `solar_wind` and `imf` are actually revised has not been measured, and the fix should not be
  designed before it is.
- **AUD-017** Email verification is still never enforced: no route consults `email_verified`, so the
  flag is recorded and never read.

## Operational

- No ID. `NASA_API_KEY` was never rotated after it was found in plaintext in a log line, and that is
  a decision rather than an oversight: `api.data.gov` has no self-service revocation, so issuing a
  new key would not disable the old one and the exposure would be unchanged. The redaction shipped
  in `df07971`. It stays open because the old key is still live and only NASA can retire it.

## Measurement

- No ID. Early degradation below the alerting floor is not detectable by rate alone. The throughput
  check added 2026-08-22 catches a source that stops delivering, but the first hour of the ISS
  slowdown on 2026-08-18 delivered 696 of 720 samples, 96.7%, and the healthy fifth percentile for
  that source over 228 measured hours is 97.5%. There is no gap between them to put a threshold in,
  so the earliest hour of a degradation is structurally invisible to any count-based rule.
  **We are counting outcomes, not measuring how long they took.** The upstream had already slowed
  from a 0.055s median to about 1.15s by that hour, which is a twentyfold change and unmissable in
  latency, while the count moved by three percent. Closing it needs the poller to record per-request
  duration, a percentile per source per window somewhere a check can read, and a threshold on the
  shape of that distribution rather than on a total. That is a backend change and a new metric
  surface, not another rule in a shell script, which is why it is here rather than done.

## Open audit findings

Reconstructed 2026-08-30 by reading `git log` since the audit's base tree against the code as it
stands. Every finding in `docs/AUDIT-2026-08.md` now carries a resolution line and
`docs/AUDIT-INDEX.md` carries a status column, so this section is the tracked half: what is still
open, one line each. A finding whose remainder is already stated elsewhere in this file is not
repeated here.

- **AUD-004** Webhook delivery is unchanged and is still an SSRF primitive: prefix-only URL
  validation, reqwest's default redirect policy, no address check, and the status code and error
  string still returned to the account owner. The highest severity item still open.
- **AUD-008** `get_events_page`, behind `GET /api/events`, was never scoped to the caller, so every
  account's custom rule anomalies, names and thresholds included, still reach every authenticated
  caller. `/api/anomalies` and the MCP tool were fixed in `475ffc1`; this route was missed.
- **AUD-009** No `limit_req_zone` exists in `frontend/nginx.conf`, so the sign in backoff added in
  `504bb5b` is per account only and an attacker spreading attempts across accounts from one address
  meets nothing at the edge.
- **AUD-011** Three backend advisories are open and `cargo audit` exits non-zero: quinn-proto
  `RUSTSEC-2026-0185` at CVSS 7.5, rkyv `RUSTSEC-2026-0235` which is blocked on duckdb, and h2
  `RUSTSEC-2026-0258`, published 2026-08-17 and therefore newer than the audit, in the live request
  path through reqwest, hyper and axum.
- **AUD-014** The forecast band is still uncalibrated epistemic spread with no observation noise
  term and no coverage measurement anywhere, while six files still label it a 95 percent confidence
  interval. Coverage is computable today from stored rows and has never been computed.
- **AUD-015** Residual only: with Kp padding gone, `lag_1` through `lag_7` and the two rolling
  features still fall back to `0.0` at the oldest end of every window, 30 cells of 304. Closing it
  means requesting `seq_len + 7` readings, not another default.
- **AUD-016** `register` still validates neither password length nor email shape, while
  `change_password` and `reset_password` both enforce a minimum, so an account can be created with
  a password it cannot later be changed to.
- **AUD-018** Two halves remain after `6b3d885`: enabling or disabling TOTP does not bump
  `token_version`, so a session an attacker holds survives the countermeasure taken against them;
  and `PurposeClaims` carries no version, so a used reset link stays replayable for its full TTL.
- **AUD-019** Email is still stored and compared verbatim everywhere except the OAuth path, which
  lowercases, so the duplicate-account trap is intact and a reset issued for one casing does not
  reach the other row.
- **AUD-020** The OAuth `nonce` is still generated and never compared, with no cookie and no PKCE,
  so the state token proves the server issued some state and not that the callback belongs to the
  browser that began the flow.
- **AUD-021** `uptime_pct` still cannot represent an outage: `backend_api` is a literal written by
  the process being measured, the denominator is rows present rather than samples expected, and the
  bucket is a rolling offset from request time rather than a calendar day. `2623cf6` answered the
  gap half the other way on purpose, so this needs a policy decision before code.
- **AUD-022** `kp_forecast` is still keyed on target time alone with no horizon and no issue time,
  and the poller still stores the 3 h mirror and discards the other three horizons, so the stored
  history and the accuracy metrics cover one horizon while the page shows four.
- **AUD-023** No retention pass exists. Eleven tables still grow without bound and no `CHECKPOINT`
  reclaims the WAL.
- **AUD-024** `neo_close_approaches_raw` still filters on `fetched_at`. Unlike the xray half this
  is not a one-line substitution: `neo` has no observation instant, only a forward-dated
  `close_approach_date`, so it needs a decision about what the window means.
- **AUD-025** The `developer` gate on CSV export is still a formatting gate, because
  `/api/reports/kp` and `/api/reports/solar-wind` return the same rows ungated, and
  `asteroid_approaches` still counts a forward window inside a card describing the past one.
- **AUD-026** Beyond Stage 3 above: there is no `cap_drop: [ALL]` and no `read_only` on any
  service, and `depends_on` is still the short form, so `condition: service_healthy` is absent and
  the backend can still start before ml has loaded its checkpoint despite ml having a healthcheck
  to wait on.
- **AUD-027** `Referrer-Policy` was named in the fix and never added. Confirmed absent from the
  live response 2026-08-30, where the other four headers and the report-only CSP are present.
- **AUD-028** Email alerts still fire from the newest stored reading with no age bound, and still
  mark the cooldown before dispatch against a send whose outcome is discarded, so a stalled feed
  re-alerts hourly and a failed send is recorded as delivered.
- **AUD-029** `/api/usage` still reports `"scope": "api_key"` as a literal on the line above the
  correctly computed `caller`.

## Enumeration coverage

Found 2026-08-30 while checking whether the shape that let `poller/anomaly` sit unmapped appears
elsewhere. A list built from what has spoken cannot contain what has never spoken.

- No ID. The NOAA space weather alerts feed has no freshness entry. `SERIES_FRESHNESS` declares 11
  components, `starlink` and `forecast` are covered separately as `celestrak` and `ml_forecast`,
  and `space_weather_alert` is covered by nothing, so `/api/alerts` going dead is invisible to the
  status page, to `component-check.sh`, which enumerates from that payload, and to the throughput
  rule in `poller-check.sh`, where `[poller/alerts]=""` disables it. Nothing would report it.
- No ID. The interval table that `poller-check.sh` enumerates from is parsed out of the backend's
  `poller: intervals loaded` boot line, which is a hand-written `info!` listing 15 fields against
  16 spawned pollers. `poll_health_snapshots` is absent from it, from `PollConfig`, and therefore
  from the env override convention `CLAUDE.md` documents. No test asserts the boot line covers
  every poller, which is the same gap one level below the mapping test that now exists.

## History

- No ID. **The audit's stated baseline sha does not exist in this repository.**
  `docs/AUDIT-2026-08.md` names its scope as commit `6f3a9d5`; that sha is not reachable from any
  ref and survives only as a loose object, because the history was rewritten after the audit was
  written. Its tree is byte-identical to `03df0f6`, which is the parent of `f70c4be`, the first
  audit fix, so **`6f3a9d5` maps to `03df0f6`** and the status reconstruction of 2026-08-30 was
  driven from `03df0f6..HEAD` with no ambiguity. Recorded rather than fixed: rewriting the report
  header would leave the same problem for the next sha anyone wrote down before a rewrite. The
  general consequence is the one worth carrying, that a sha quoted in an untracked document is only
  as durable as the history it names, and that a document written against a tree should name the
  tree it can prove rather than a commit that may be rebased out from under it.
