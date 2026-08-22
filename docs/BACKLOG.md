# Backlog

Work that was found, understood, and deliberately not done. One line per item.

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

## Process

- No ID. Nothing records which of the 29 audit findings are closed. `docs/AUDIT-2026-08.md` carries
  a `**Resolved**` marker on exactly one, AUD-013, and `docs/AUDIT-INDEX.md` has severity and a file
  location but no status column, so the true state can only be reconstructed by reading git history
  against each finding one at a time. Deferred because it is a day of careful archaeology rather
  than a change, and listed here because that is exactly the kind of work that otherwise never gets
  written down and never gets done.

## What could not be accounted for

This file is seeded only from deferrals that were stated explicitly at the time the decision was
made. It is not a complete list of open audit findings and should not be read as one, because
nothing currently records which findings are closed. That gap is itself an item, under Process
above.
