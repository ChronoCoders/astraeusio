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

- No ID. **`component-check.sh` has no duration term, so a feed dead for twelve hours and one dead
  for twelve days look identical after the first mail.** It mails once per distinct set of bad
  components and then stays silent while that set is unchanged: no re-mail after a threshold, no
  severity step, no separate treatment for a component that has been degraded for a week. Observed
  2026-09-01, when `noaa_imf` and `noaa_solar_wind` had been alerting continuously since 31 Aug
  21:07 UTC, about 17 hours, on one mail sent at the transition.
  The silence is deliberate and right for the first hour, since re-mailing every fifteen minutes is
  how an alert gets filtered to a folder. What is missing is the other end: something that says a
  known problem has now lasted long enough to be a different problem. Candidates are a second mail
  at a duration threshold, a daily digest naming what is still bad and for how long, or including
  the age in the recovered mail so the record shows the length. Wanted, not tonight, and not mixed
  into the status page work.

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

- **AUD-033** **The four horizon heads are one function with four amounts of shrinkage.** Added
  2026-09-01, measured on the checkpoint trained after the AUD-032 fix, so it is not an artefact of
  the wrong leads. Every head's correlation with the outcome peaks at a lead of three hours,
  including the head sold as 24h, and the four profiles have nearly the same shape. At a lead of 24
  hours the 3h head correlates better with the outcome, 0.169, than the 24h head does, 0.164.

  What differs between heads is only shrinkage: sd(pred)/sd(obs) of 0.81, 0.67, 0.51, 0.32 across 3h
  to 24h. The model produces one estimate of where Kp goes next and damps it more for longer labels.
  That still beats persistence at 24h by 0.202, because heavy shrinkage is close to right there, but
  the long horizons carry no information the short one does not, while the product presents four
  independent forecasts.

  Not proposed as a fix. Candidates: separate trunks or per-horizon models, a longer input window
  since 16 slots is 48 hours for a 24 hour lead, features carrying solar-wind lead time rather than
  Kp history alone, or publishing fewer horizons.

- **AUD-030** **Superseded 2026-09-01 by AUD-032.** This finding said the forecast loses to
  persistence at three hours. The comparison evaluated each head at its labelled lead while every
  head is trained one period further out, so the model was answering a harder question than the
  baseline. At the lead it was actually trained for, the same checkpoint scores 0.805 against
  persistence 0.882 and a two-parameter fit 0.826, which is a win rather than a loss. **Do not quote
  the numbers from this entry, and the bar it recorded is withdrawn**, since it was computed against
  a mislabelled model.

  What survives is narrower and is about labelling rather than skill: at the horizons the product
  advertises, a two-parameter fit on the last observation beats the model on the storm-rich
  walk-forward window, 0.669 against 0.680 at 3h and 0.826 against 0.854 at 6h, measured with the
  same expanding-window fold structure. That claim is carried forward under AUD-032 and re-measured
  after the index fix.

- No ID. **The bar for the 2026-09-01 retrain**, re-derived after AUD-032 and recorded before the
  run so it cannot move afterwards. Everything below is measured **at matched leads**: model,
  persistence and the linear fit all answering the same horizon. The previous bar is withdrawn,
  because its model column was measured against a mislabelled head.

  The baselines never involved the model, so they carry over unchanged, and they are the bar.

  Storm-rich walk-forward window, expanding-window fit, fold evaluation, 4 folds, 5840 slots with
  299 at Kp >= 5. Persistence **0.684 / 0.882 / 1.061 / 1.228** and the two-parameter fit
  **0.669 / 0.826 / 0.958 / 1.054** at 3h / 6h / 12h / 24h. This is the window that decides, because
  it contains the storms.

  Out-of-sample window, 375 held-out windows, 2026-07-14 to 2026-08-29, quiet. Persistence
  **0.581 / 0.696 / 0.857 / 1.009** and the two-parameter fit **0.568 / 0.670 / 0.797 / 0.923**.

  A retrained model has to beat the two-parameter fit on the storm-rich window at the horizon it is
  published as. Beating it only on the quiet window is what happened last time and it did not
  survive contact with the larger sample.

- **AUD-031** **The model loses to persistence at both ends of the Kp range, and storms are 1.9
  percent of the training set.** Added 2026-09-01. Model MAE minus persistence MAE by observed Kp,
  in sample at 3h: -0.067 in the 0 to 2 band, +0.188 at 2 to 3, +0.085 at 3 to 4, -0.124 at 4 to 5,
  **-0.656 at Kp >= 5**. At 24h the storm gap is -0.811. It wins only in the middle band, and this
  is on data it trained on, so it is a fitting failure rather than a generalisation failure.

  The cause is the training distribution meeting a squared-error loss on the Kp level: 1119 of 59296
  slots are at Kp >= 5, 1.9 percent, and 317 at Kp >= 6, 0.5 percent, so the gradient from the quiet
  bulk decides the fit. Measured prediction ranges, out of sample: 0.80 to 5.35 at 3h and 1.18 to
  3.44 at 24h, against observations reaching 7.33. Beyond twelve hours the model cannot emit a
  storm-level number at all.

  Deliberately out of scope for the 2026-09-01 retrain, which changes the target parameterisation
  only, because mixing the two would make neither attributable. Candidates: activity-weighted
  sampling, an asymmetric penalty on under-forecasting, or a separate storm-regime model. Each needs
  measuring against persistence conditional on Kp >= 5 rather than marginally.

- **AUD-009** No `limit_req_zone` exists in `frontend/nginx.conf`, so the sign in backoff added in
  `504bb5b` is per account only and an attacker spreading attempts across accounts from one address
  meets nothing at the edge.
- **AUD-011** Two backend advisories keep `cargo audit` exiting non-zero after h2 was fixed in
  `164db2b`: quinn-proto `RUSTSEC-2026-0185` at CVSS 7.5 and rkyv `RUSTSEC-2026-0235`. Neither is
  compiled, and `.cargo/audit.toml` records why they are not equally safe: quinn-proto has no path
  into the build, while rkyv's parent rust_decimal is compiled and only its `rkyv` feature is off,
  so a feature change on duckdb's side is enough to make it live. Whether to ignore them, and on
  which of those two arguments, is an open decision.
- **AUD-014** The forecast band is still uncalibrated epistemic spread with no observation noise
  term, while six files label it a 95 percent confidence interval. Coverage was computed for the
  first time on 2026-08-31: **13.1 percent** over 1229 forecasts paired with the observed
  three-hour Kp, mean width 0.405 Kp against a mean absolute error of 0.727 Kp, so the typical
  error is nearly twice the width of the band. Closing it means an observation noise term and
  recalibration, then the label. `ml/test_serve.py` pins the construction and deliberately does not
  assert coverage, since no unit test can turn 13.1 into 95.

  **The label half is closed** in `1915ea3`: eleven files across two languages stopped calling it a
  95 percent confidence interval, the band came off every marketing surface, and it survives on the
  Forecast page and in the API as model spread with the measured coverage stated beside it.
  `ci_lower` and `ci_upper` keep their names because callers depend on them.

  **The calibration half was sized on 2026-09-01 and deliberately not shipped.** Conformal
  calibration by replay, calibrating on 2026-05-04 to 2026-07-14 and measuring on the disjoint
  2026-07-14 to 2026-08-29, reaches 94.7 / 94.7 / 96.5 / 98.4 percent marginal coverage across the
  four horizons, at a median half width of **±1.29 / ±1.54 / ±2.17 / ±2.54 Kp** against ±0.17 today.
  It fails where it matters: conditional on observed Kp >= 5 its coverage is 62 / 38 / 50 / 50
  percent, against 0 percent for the band shipping now. A band that is 95 percent overall and half
  that during storms is confident exactly when it is wrong, so no calibrated band is published. Only
  8 storm slots and 22 active slots fall in the held-out window, so those figures are directional;
  the direction is consistent across all four horizons and both conformal variants. Closing this
  properly needs conditional calibration with a storm sample that does not exist yet, or a variance
  head validated on active conditions specifically.
- **AUD-015** Residual only: with Kp padding gone, `lag_1` through `lag_7` and the two rolling
  features still fall back to `0.0` at the oldest end of every window, 30 cells of 304. Closing it
  means requesting `seq_len + 7` readings, not another default.
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

Both items found here are closed. Kept because the shape recurs and the reasoning is worth having
next to the next instance of it.

- Closed `8029954`. The `poller: intervals loaded` boot line was a hand-written `info!` naming
  fifteen pollers against sixteen `tokio::spawn` calls, with `health` missing, so the one external
  check that enumerates pollers from something other than the log could not see it. The line is now
  generated from `PollerConfig::intervals`, `health` has an entry and a `HEALTH_INTERVAL` override
  like every other poller, and `every_spawned_poller_is_in_the_interval_table` reads the spawns out
  of the source file and fails when a poller exists with no entry, or an entry with no poller. A
  second test holds the rendered line to the `name=integer` shape `poller-check.sh` parses, since
  that line is an interface and not a debug aid. Verified on the host after deploy: the check now
  enumerates sixteen pollers and `--selftest` passes.
- Closed. The NOAA alerts feed now has a watcher. It could never have a freshness threshold, because
  alerts are episodic and no row age separates a quiet sun from a dead feed, so it is watched on the
  verdict its poller records each cycle rather than on the age of what it stored: `POLL_LIVENESS` in
  `db.rs` declares the component, `poll_alerts` writes operational or degraded every 300 s,
  `/api/health` publishes it beside the series components, and the status page carries a row for it.
  A verdict older than 1800 s reads as degraded rather than repeating the last good answer, which is
  what stops a stopped poller looking healthy forever.

  The horizon in the third part was measured, not chosen: over 2026-04-10 to 2026-08-30, 142 days
  and 491 gaps between consecutive products, the longest quiet stretch was 97.8 h, p99 62.6 h,
  median 1.68 h, with 32 gaps over a day, 11 over two and 4 over three. Seven days is 1.7 times the
  longest ever observed. Worth knowing it rests on four samples past 72 h from one stretch of one
  solar cycle, and quiet periods lengthen towards solar minimum, so it should be re-derived from a
  year of data.

- Closed. **The status page enumerated its components by hand.** `StatusPage.jsx` held a literal
  `COMPONENTS` array and rendered only those rows, so a component `/api/health` published and the
  array omitted was silently not displayed, on the page whose whole job is to make things visible.
  The third instance of this shape, after the `poller/anomaly` mapping and the interval boot line,
  and the only one a user could see. It now renders what the payload contains: `ORDER` survives as a
  display hint applied over the payload rather than as the source of what exists, an unrecognised
  component is appended instead of dropped, and its label is derived from its key with a humanised
  fallback, so a component published before its locale strings land reads as `NOAA Alerts` rather
  than vanishing or rendering a raw i18n key. `ORDER` is still the skeleton when `/api/health` is
  unreachable, because a blank page is the worst answer at the moment somebody is looking at it.

## Process

- No ID. **Every security fix is public before it is live.** `deploy.sh` deploys what it finds at
  `origin/main`: it runs `git fetch origin`, selects services from `git diff HEAD..origin/main`,
  then `git pull --ff-only`. So the push to a public GitHub repository is a precondition of the
  deploy, not a step that could be reordered, and the window lasts as long as the build. On
  2026-08-31 the webhook SSRF fix `fedbde7` was pushed at 22:01 and running at 22:27, twenty five
  minutes during which a public commit named a live hole in production and its message described
  how to reach it.

  The message is the smaller half. A terse subject would not have helped much, because the diff
  itself is legible: an address predicate and `redirect::Policy::none()` appearing in a webhook
  module say what was wrong without a sentence of prose. Anything that only edits the message
  treats the readable part and leaves the code.

  Three ways out, none taken yet:

  - **A message that says nothing until it is deployed**, with the explanation added afterwards.
    Cheapest, and the weakest, for the reason above. It also depends on somebody remembering to
    come back, which is the failure mode this file exists to record.
  - **Deploy from a local bundle** rather than from `origin`, pushing to GitHub after the health
    checks pass. Closes the window for the code and the message together, needs no new
    infrastructure and no new credentials. The cost is that `deploy.sh` would no longer be able to
    say the deployed sha is `origin/main`, so production could drift ahead of the public repository;
    that is worth accepting only if the deploy pushes on success and fails loudly if it cannot.
  - **A private mirror** that the host pulls from, with GitHub pushed afterwards. Same guarantee as
    the bundle, but it adds a second remote to keep in sync and a new way to deploy the wrong thing.

  **Preference: the bundle, with a push to `origin` on success.** It fixes the property rather than
  the prose, adds nothing to maintain, and the drift it introduces is the one risk in the list that
  a script can check for itself at the end of a run.

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
