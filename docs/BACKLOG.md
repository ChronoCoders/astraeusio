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

## Operational

- No ID. `NASA_API_KEY` was never rotated after it was found in plaintext in a log line, and that is
  a decision rather than an oversight: `api.data.gov` has no self-service revocation, so issuing a
  new key would not disable the old one and the exposure would be unchanged. The redaction shipped
  in `df07971`. It stays open because the old key is still live and only NASA can retire it.

- No ID. **Tracking the ops scripts relaxed them from 700 to 755 on the host, and git cannot put
  that back.** The thirteen scripts lived on the host at mode 700 before they were tracked in
  `1bbf5c5` and `c4182bd`. Git records one bit, executable or not, so a tracked file arrives at 755
  under the default umask and every pull restores 755. Nothing in them is secret: the per-file audit
  on 2026-09-01 found no embedded credential, every one reads from `.r2-s3-key` or `backend/.env`,
  both of which stay untracked at 600. This sentence also named `.cloudflare-key` until 2026-11-26,
  when AUD-073 found that no tracked script reads it. So this is not an exposure, it is a
  property that changed quietly as a side effect of a change made for a different reason, and it is
  recorded because nobody chose it.
  If 700 is the mode these should have, the tracked copy cannot enforce it and something else has
  to: a `chmod` in whatever runs the pull, or an assertion in `component-check.sh` that fails when a
  mode drifts, which has the advantage of noticing rather than silently correcting. The third option
  is to decide 755 is correct for a directory holding no secrets and write that down, which is
  cheapest and is the current state by accident rather than by decision. Unresolved because the
  answer is a preference about the host, not a defect.

- No ID. **The free tier advertises a sixty second data delay and nothing implements it.**
  `lib/plans.js` gives free the features `req100day`, `delay60` and `kpSolar`, and
  `pricing.features.delay60` reads "60s+ data delay". No delay exists anywhere in the backend:
  `grep` for one across `routes.rs` and `plan.rs` returns nothing, and every read handler serves
  the newest row it has to every tier. So the pricing page claims a limit the product does not
  apply, which is the same class of defect as AUD-025 and larger than it: the CSV gate was a proxy
  for this distinction, and dropping the gate leaves the real one still missing.
  This is a product decision before it is code. Implementing it means serving two versions of the
  same series, which is why it is recorded rather than done.

  **AUD-012 closed 2026-09-22**, the other way, in `98282e0`. The claim came off rather than the delay
  going in: `delay60` is gone from `lib/plans.js`, the delay and real-time rows are gone from the
  comparison table, and the eight keys behind them are gone from both locales. The product applies
  no delay and no longer says it does. Serving two versions of the same series remains an option
  nobody has chosen, not an outstanding defect.
- No ID. **Decided 2026-11-24: caddy comes off the host rather than staying disabled.** AUD-056
  left it installed, disabled, holding an expired certificate it cannot renew. Disabled is one
  `systemctl enable` away from repeating the outage, and nothing in this repository can see the
  difference, so the package goes.

  Two things happen before the removal, recorded here so the step is a lookup rather than a
  rediscovery. Confirm `/etc/caddy/Caddyfile` defines no site other than `astraeusio.com`, because
  removing the package takes every site it serves with it and this host was not surveyed for a
  second one. Then copy the whole of `/etc/caddy` into the server's backup location, not only the
  Caddyfile, since the certificate state and any drop-in under `conf.d` are part of what is being
  destroyed.

  That work is step 5 of the order recorded under Process and is not done here. Only the decision is.


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

## Who the users are

Established 2026-09-04 by reading the `users` table on the host. The account list is no longer
four addresses that all belong to us.

| Address | Plan | Verified | Origin | Created |
|---|---|---|---|---|
| `contact@chronocoder.dev` | enterprise | yes | password | 2026-05-10 |
| `altug@bytus.io` | free | yes | password | 2026-05-14 |
| `891483383@qq.com` | free | **no** | password | 2026-05-29 |
| `deploy-verify@astraeusio.com` | free | yes | password | 2026-08-10 |
| `deploy-verify-dev@astraeusio.com` | developer | yes | password | 2026-08-10 |
| `dystek12@gmail.com` | free | yes | github | 2026-08-27 |

Two of the six are strangers: `891483383@qq.com`, who signed up with a password and never
confirmed the address, and `dystek12@gmail.com`, who arrived through GitHub OAuth and is
verified because the provider vouched for the address. Neither is known to us.

This ends the assumption that every schema change, session invalidation and account lockout has
been priced against, which was that the whole user table is ours and any breakage is ours to
absorb. From here a migration that rewrites user rows, a change that invalidates tokens, and any
path that locks an account have to be weighed against a third party being on the other end of it.
The two deploy accounts stay the exception: they are named literals in `DEPLOY_ACCOUNTS`, and the
`2026-09-02-verify-deploy-accounts` migration only touches those two addresses, which is the shape
the next user-table migration should copy rather than a bare `UPDATE users`.

What it changes about findings that are still open:

- **AUD-009**, no `limit_req_zone` at the edge, changes character. It was filed as a gap in
  brute-force defence. The per-account backoff it leaves standing is also a lockout primitive:
  six wrong passwords against an address that is not ours takes that person's account away until
  the wait expires, and nothing at the edge limits who can spend those six. The finding is now a
  denial of service against a third party as much as it is a weak defence for us.
- **AUD-027**, the missing `Referrer-Policy`, now concerns other people's credentials rather than
  only ours. `resend_verification` builds the link as `{app_url}/verify-email?token={token}` and
  the reset link is the same shape, so a single-use secret rides in the query string, and it is
  the credential an unverified stranger's only way back depends on. Measured on the live route
  2026-09-04 before the header was added: every subresource `/verify-email` loads is same-origin,
  the rendered tree is `Navbar` plus a status card with no third-party link, and the edge injects
  no beacon, so nothing was leaving. The exposure was latent rather than active, one added font,
  analytics tag or error reporter away from real.
- **AUD-020**, OAuth PKCE left out deliberately, stops being hypothetical. There is a real
  OAuth-origin account now and it is not ours, so the party exposed by the remaining gap is the
  stranger rather than a test account we control.
- The remaining open findings, **AUD-011**, **AUD-014**, **AUD-015**, **AUD-022**, **AUD-026**,
  **AUD-030** through **AUD-033**, touch dependencies, the model or the containers and hold no
  per-account data, so the change does not reach them.

### AUD-017 was considered for rollback on a reading that did not survive measurement

On 2026-09-04, with a stranger's account sitting unverified, the working assumption was that
`email_verified` had locked a real user out and that the gate should come back off until the
recovery path was proven. It had not. Reading the four `verified_gate` call sites: it gates
creating an API key, creating a webhook, creating a custom rule, setting email alert thresholds
and changing plan, and the first four are separately plan-gated at developer, pro, enterprise and
developer. The account in question is on `free`, so the only capability the gate removes that its
plan does not already remove is `POST /api/user/plan`. It can still sign in and still read
everything its plan allows, which is exactly what `verified_gate`'s own doc comment says it is
for.

The gate stays. Rolling it back would have removed the control for every account in order to
return one capability to one person. Recorded because the wrong reading was the urgent-sounding
one, and what settled it was reading the call sites rather than the finding's title.

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

- No ID. **A verification email to an address that once hard bounced is dropped by the provider,
  and `/auth/resend-verification` still answers 204.** Found 2026-09-04 while proving the AUD-017
  recovery path end to end. `resend_verification` was deliberately hardened so that a 204 means
  the provider took the message rather than that the work was queued. Resend takes it, matches the
  recipient against its own suppression list, and drops it. The API still returns success, the
  backend logs `mailer: "Verify your Astraeusio email address" sent to ...`, and the caller is
  told the mail is on its way when nothing was sent.

  Sourced. `GET https://api.resend.com/emails/5a3ee458-557a-4466-91b6-4babc3852a19`, the send made
  at 14:08:18Z, carries `last_event: suppressed`. `GET /suppressions` holds exactly two entries,
  both `origin: bounce`: `deploy-verify@astraeusio.com` from 2026-08-10 03:10:58Z and
  `deploy-verify-dev@astraeusio.com` from 2026-08-10 19:43:17Z. The matching bounces are in the
  send log at 2026-08-10 03:10:57Z and 19:43:15Z. Both accounts were created against a domain that
  had no inbound routing for those addresses, so their first verification mail hard bounced and
  the provider has refused them ever since.

  **`891483383@qq.com` is not suppressed.** That listing is complete, `has_more: false`, two of
  two, and it is trustworthy because it contains the two addresses whose bounces were confirmed
  independently in the send log. So the one locked out stranger is not blocked by this. What
  reaches them is still unproven, because no send to that address appears anywhere in the 120
  records the log retains, which reach back only to 2026-08-08.

  The shape of the defect is that acceptance was treated as delivery one level too shallow. A
  synchronous 200 from the provider cannot see a suppression, a bounce or a spam placement, all of
  which arrive later as events. Closing it means consuming Resend's `email.bounced` and
  `email.delivery_delayed` webhooks, recording the last delivery outcome per address, and refusing
  to claim a send that the provider will not make. Until then any account whose address bounces
  once is permanently unable to recover, and is told the opposite.

  **The synchronous half is closed.** `ResendSender::deliver` asks
  `GET /suppressions/{email}` before every send and returns a three-state `SendOutcome` instead of
  a `bool`, so a caller can tell "nothing was sent and retrying is pointless" from "the attempt
  failed". The check sits behind the `Sender` trait rather than at the call sites, because the
  thing being protected is the claim that mail was sent, that claim is a `SendOutcome::Sent`, and
  only `deliver` can construct one. `only_deliver_can_claim_a_message_was_sent` and
  `no_caller_outside_the_mailer_constructs_an_outcome` hold that shape, so a sixth mail path added
  later is covered by existing rather than by somebody remembering.

  `resend_verification` answers 422 `address_rejects_mail` and names `hello@astraeusio.com`. The
  procedure behind that copy is in `docs/RUNBOOK.md` under "An account cannot receive our mail",
  written in the same change: copy that points at support without support knowing what to do is
  worse than no copy.

  **A lookup that cannot be answered sends anyway, deliberately.** One attempt, two second
  timeout, no retry. Refusing to send because the provider is unreachable would put a false
  failure on the recovery path in place of the false success, and the user has no way to tell one
  from the other or to act on either. It is also what the code did before this check existed, so
  an outage degrades to the old behaviour rather than to a new one. The `Unknown` case logs, since
  "we did not check" and "we checked and it was clear" must not look the same afterwards.
  `an_unanswered_lookup_does_not_refuse_the_send` pins it, and is the weakest test in that file:
  the branch only runs with a network, so it asserts the shape of the source rather than the
  behaviour.

  **Registration was left alone, as a decision rather than an oversight.** A suppressed address at
  sign up still answers 201 and still tells the reader to check their mail. That is not where the
  question gets asked: they look, find nothing, and press resend, and resend is the endpoint that
  answers. Putting the answer in both places would be two things to keep in step for one fact. The
  send is logged so it is not invisible.

  **Password reset keeps its 204 and stops discarding the outcome.** The unconditional 204 is
  deliberate and stays, because a status that varied with whether the address exists is an account
  enumeration oracle. That argument only ever covered the status code, never throwing the result
  away. Stating the limit plainly: nothing aggregates the new log line, so "an operator can see
  it" means "it is in the backend log". A per-address record that something could alert on is the
  durable half and it does not exist yet.

  **What is still open under this finding** is the webhook: `email.suppressed`, `email.bounced`,
  `email.complained` and `email.delivery_delayed`, Svix signature verification over the raw body,
  a per-address outcome store, and `deliver` returning the provider message id so an event can be
  joined back to a send. The pre-check stops the endpoint lying in the moment; only the webhook
  makes the system know.

  **Three paths, one defect, one `Sender`.** All four mail paths run through the same
  `ResendSender::deliver`, which maps `Ok(_)` to `true`, and Resend answers `Ok` for a suppressed
  recipient. They differ only in how much of the outcome they keep, and every one of them is wrong
  under suppression:

  | Path | Returns | Caller checks it | Consequence |
  |---|---|---|---|
  | verification, `auth.rs:600` | `bool` | yes, 502 on false | 204 claims a send that never happens |
  | password reset, `auth.rs:1056` | `()` | cannot, discarded | same, and blind to genuine provider errors too |
  | email alerts, `poller.rs:846` | `bool` | yes, gates the cooldown | cooldown recorded, an hour of silence for an alert nobody got |
  | welcome, `auth.rs:561` | `()` | no | silent, and harmless |

  The alert case is the same defect the code's own comment says it fixed. It stopped marking the
  cooldown before the send; it still marks it on a send the provider will not make. Password reset
  is the worst of the three, because `send_password_reset_email` returns `()` and there is no bool
  to check even before suppression enters it. Its 204 is deliberate and must stay, since a 502
  there would be an account-existence oracle, so the fix for reset is to stop discarding the
  outcome rather than to change the status code.

  **Order of work, decided 2026-09-04.** The synchronous pre-check first, the webhook after. A
  webhook event arrives after the response has already gone out, so it can never make that response
  truthful in the moment; only a check before sending can. `GET /suppressions/{email}` is confirmed
  to exist and to be O(1): 200 with the suppression object for `deploy-verify@astraeusio.com`, 404
  `Suppression not found` for `altug@bytus.io` and `891483383@qq.com`, which is also a positive
  control on those two 404s. Note that `GET /suppressions?email=...` is **not** a filter; it
  ignores the parameter and returns the whole list, so nothing should be designed around it.

  The webhook half needs an endpoint outside the JWT extractor, Svix signature verification over
  the raw body using the `svix-id`, `svix-timestamp` and `svix-signature` headers and a secret from
  the webhook's dashboard page, a per-address outcome table, and `deliver` returning the provider
  message id instead of `bool` so an event can be joined back to a send. Resend publishes a
  dedicated `email.suppressed` event alongside `email.bounced`, `email.complained`,
  `email.delivery_delayed`, `email.delivered` and `email.failed`, so the asynchronous half does
  report this case directly.

- No ID. **An account cannot change its own email address, so a mistyped one is unrecoverable
  without an operator.** Found 2026-09-04 while writing the copy for a suppressed address. There
  is no change-email handler anywhere in the codebase: `users.email` is the primary key, it is
  written once at registration or by the OAuth path, and nothing else ever updates it.

  The consequence only became visible when the copy had to be written. A user whose address is
  wrong or dead cannot verify, cannot reset a password, and cannot correct the address, because
  every route back runs through mail sent to the address that does not work. That is why the
  refusal for a suppressed address says "email hello@astraeusio.com and we will reset it" rather
  than "try another address": the second sentence would describe something the product cannot do.
  A support mailbox is not a fallback here, it is the only path, and it is a manual one, so every
  such account costs a person and a `DELETE /suppressions/{id}`.

  Not scoped here, and it is more than one endpoint. Changing the address means proving the new
  one before the old one stops working, which is a second verification token bound to a pending
  address rather than to the account, a decision about whether the old address is told, and a
  primary key that is currently the thing being changed. The last of those is the real cost: every
  table that references a user does so by `user_email`, so either the column stops being the key
  or the change cascades across `api_keys`, `webhooks`, `usage_records`, `email_alerts` and
  `custom_anomaly_rules`.

  Until it exists, the runbook procedure under "An account cannot receive our mail" is the
  product's only answer, and it is worth knowing that the answer scales with a human rather than
  with the user count.

- No ID. **The email verification token is replayable for its full 24 hour life, and every use
  sends another welcome mail.** Found 2026-09-04 while proving the AUD-017 recovery path. One
  token produced three welcome mails: a browser opening the emailed link posted at 17:52:57 and
  the mail went at 17:52:58, then two replays of the same token at 17:53:23 each returned 204 and
  each sent another at 17:53:27. Nothing about the token changes when it is used.

  This is the property AUD-018 gave the password reset link, and verification never got it. Reset
  is single use because `update_password_hash` bumps `token_version` in the same statement that
  writes the hash, and `decode_purpose_checked` refuses a purpose token whose `ver` no longer
  matches the account. `verify_email` writes `email_verified` and touches no counter.

  **The same mechanism is wrong here.** `token_version` is shared between purpose tokens and
  session JWTs: the session extractor rejects a session whose `ver` is behind. Bumping it on
  verification would sign the account out at the moment it verifies, and would break the page
  doing it, since `VerifyEmailPage` refreshes `/api/user/me` with the stored session token
  immediately after the 204. Signing a user out is right for a password change and wrong for the
  one action we most want people to complete.

  **Closed 2026-09-04**, in `431e561`. The update now reads
  `UPDATE users SET email_verified = TRUE WHERE email = ? AND email_verified IS NOT TRUE` and
  returns whether a row changed, through the writer to the handler. A first use answers 204 and
  sends one welcome mail; a replay answers 409 and sends nothing, the same status and wording
  `resend_verification` already gave for an address that is already proven. The welcome send is
  awaited rather than spawned, because a spawned send is unobservable and nothing could have
  asserted either half. The link lifetime is one hour, from `VERIFY_EMAIL_TTL_SECS`, used by both
  mint sites, and the mail body says one hour because a test holds the sentence and the constant
  to each other.

  **The awaited welcome send is a deliberate trade, not an oversight.** It was
  `tokio::spawn`ed, which returned the 204 immediately and made the send invisible: nothing could
  assert that a first use sends exactly one mail and a replay sends none, which is the whole
  property this change exists to add. Awaiting it puts a mail provider round trip inside the
  request. Measured from the host on 2026-09-04, five GETs to `api.resend.com`: 0.126 to 0.387 s,
  median 0.165 s. The cold connection samples are the representative ones, 0.26 s and up, because
  `ResendSender::deliver` constructs a fresh `Resend` client on every call and so never reuses a
  connection; a POST carrying a body sits at or above that. So roughly a quarter to a third of a
  second added to `POST /auth/verify-email/{token}`, once per account, forever.

  Accepted because the endpoint runs once in an account's life and an untestable property is worth
  less than a third of a second. The send's result is deliberately ignored: a welcome mail that
  does not go out must not turn a verification that did work into a failure the user sees. If that
  latency ever matters, the answer is a queue with an observable seam, not a bare spawn, since a
  bare spawn returns the property to being unassertable.

  **What it does not give, and the condition that would make that matter.** It tests the current
  state rather than spending the credential. The token itself is untouched by being used: what
  stops the second use is that the row is already `TRUE`, not that the link is dead. So for as
  long as the token lives, it remains a working key to a lock that simply happens to be open
  already. Nothing in the product can close that lock again, since no path un-verifies an account
  and only a direct database edit can, which is why this was the right trade rather than a column
  and a migration. **If a path is ever added that un-verifies an account**, whether an address
  change, an admin action, or a bounce handler marking an address unusable, that assumption dies
  the day it ships and a stale link inside its hour becomes usable again. That is the point at
  which a purpose scoped counter beside `token_version` stops being over-engineering and becomes
  the fix. It cannot be `token_version` itself: that counter is shared with session JWTs and
  bumping it would sign the account out at the moment it verifies.

  **The welcome mail should not fire on a repeat regardless of how the token is fixed.** It is the
  amplification: one captured link is one mail per request, unauthenticated, for 24 hours. A
  welcome mail to somebody already verified is also a spam complaint waiting to happen, and a
  complaint puts the address on Resend's suppression list, which is exactly what leaves an account
  unable to recover in the finding above. The two defects feed each other.

- **AUD-034** **Five queries use a row count to mean a duration, while the cadence that makes the
  two equal is env-overridable.** Found 2026-09-22 while looking for other constants with the shape
  of the backup floor. `db.rs:2246` reads `FROM kp ORDER BY observed_at DESC LIMIT 1440`, and 1440
  rows is a day only because Kp arrives once a minute. The same at `db.rs:2345` solar_wind,
  `db.rs:2420` imf, `db.rs:2444` dst, and `db.rs:2323` kp_3h, where `LIMIT 240` is thirty days only
  because that series is three-hourly.

  Every one of those cadences is settable from the environment: `poller.rs::PollConfig::from_env`
  reads `KP_INTERVAL`, `SOLAR_WIND_INTERVAL`, `IMF_INTERVAL` and the rest. Double `KP_INTERVAL` to
  cut API pressure, which is exactly what that knob is documented for, and the chart silently
  becomes two days of data while the code, the API and the axis label all still say one. Nothing
  connects the constant to the interval, and nothing fails.

  Same shape as the backup floor: a value that is correct until a different change moves what it
  measures. The difference is that the backup floor announced itself twice a day for nineteen days
  and this one would announce nothing at all, because a chart with the wrong window looks exactly
  like a chart with the right one.

  Not fixed here. The fix is to express the window as a duration and let the query derive the row
  count, or to select on `observed_at > now - 86400` and drop the count entirely, which is what the
  range queries at `db.rs:770` and `db.rs:795` already do.

- **AUD-035** **Local backup retention is a count, and nothing checks the disk it costs.**
  `backup.sh:29` keeps `KEEP=7` files. What that occupies is set by the database size, which nobody
  measures on this path. At the pre-rebuild 1.1G per file it was 7.7G held on a 79G volume; today
  the same seven files are 879M. The footprint moved by a factor of nine without the constant
  changing or anybody deciding anything.

  No script on this host checks free space before writing a backup. `rebuild-db.sh:62` is the only
  one that reads `df` at all, and only to guard its own run. So the failure mode is that the
  database grows back, seven copies grow with it, and the first sign is a full disk rather than an
  alert.

  Measured 2026-09-22: `/` is 79G with 43G available at 44 percent used, so this is not close today.
  It is recorded because the coupling is invisible, not because it is urgent.

  Not fixed here. The candidates are a free-space check in `backup.sh` before it writes, retention
  expressed as a budget rather than a count, or `backup-check.sh` reporting headroom alongside the
  freshness it already reports.

- No ID, low risk. **`r2_upload.py:63-64` sets `multipart_threshold` and `multipart_chunksize` to a
  fixed 64 MB** against a file that has grown past it and will not shrink back below it. It is a
  transfer tuning value rather than a correctness one, so nothing breaks either side of the
  boundary. Listed with the two above because it is the same kind of constant: a size written down
  once, against something that moves.

- No ID, rides the next backend change. **`routes.rs:1004` documents a pricing claim that no
  longer exists.** The comment reads that the real free-versus-paid line the pricing page claims is
  `delay60`, a sixty second delay on free-tier data, and that nothing implements it. The claim was
  removed from the pricing page, both locales and `plans.js` on 2026-09-22, so the comment now
  describes something that is not there. Left in place deliberately: correcting a comment would
  cost a full backend rebuild and container recreation on the host, which is not a price worth
  paying for prose. It goes with whatever changes `routes.rs` next.

  **Closed 2026-11-24**, in `c01039b`, the AUD-052 fix. The three stale lines are gone from the
  `get_report_export` doc comment and `delay60` now appears nowhere in `backend/src` or
  `frontend/src`. It took two backend changes rather than one: `routes.rs` also changed under AUD-050
  on 2026-10-05 and the comment was not carried then, which is what a rider on "the next change"
  costs when nothing enforces it.

- **AUD-009** No `limit_req_zone` exists in `frontend/nginx.conf`, so the sign in backoff added in
  `504bb5b` is per account only and an attacker spreading attempts across accounts from one address
  meets nothing at the edge.
- **AUD-011** Two backend advisories keep `cargo audit` exiting non-zero after h2 was fixed in
  `164db2b`: quinn-proto `RUSTSEC-2026-0185` at CVSS 7.5 and rkyv `RUSTSEC-2026-0235`. Neither is
  compiled, and `.cargo/audit.toml` records why they are not equally safe: quinn-proto has no path
  into the build, while rkyv's parent rust_decimal is compiled and only its `rkyv` feature is off,
  so a feature change on duckdb's side is enough to make it live. Whether to ignore them, and on
  which of those two arguments, is an open decision.
- **AUD-042** A custom anomaly rule is detected and never delivered. `anomaly.rs` writes the rule's
  hit to `alerts_anomaly` with an `anomaly_type` of `custom:<id>`, and nothing carries it further.
  `webhooks.rs` accepts five event names and `custom:*` is not among them, so no webhook can
  subscribe to one. The email dispatcher evaluates only the Kp and wind thresholds. So the feature a
  customer configures produces a row in the dashboard feed and no notification, which is the one
  thing a person setting a threshold is asking for.

  **Delivery is not built here, and the reason is ownership.** A custom rule belongs to the account
  that created it, and `alerts_anomaly.user_email` carries that. Any delivery path has to scope to
  the owner on both channels, or one account's rule notifies another account's webhook. That is the
  same class as the reads audited under `ANOMALY_VISIBLE_TO`, and it is not a change to make at the
  same time as correcting copy. The plan that should carry custom rules is also undecided: the code
  gates creation at `enterprise` while the pricing page sells custom thresholds on Business.

  Corrected on 2026-09-23 in the copy only: the pricing feature list and the comparison table now
  say custom thresholds and custom anomaly rules appear in the dashboard feed, so nothing promises
  a notification that does not arrive. Multi-channel alerts remain listed, because webhooks and
  email do work for the five built-in event types.

- **AUD-043** Every poller sleeps after its work, so no poll rate derived from an interval constant is
  exactly achievable, and every alert target built from one is wrong by the fetch time. The mechanism
  is real. In steady state its magnitude is below every threshold, and the figures first recorded here
  overstated it by a factor of about thirty.

  `poll_iss` and its fifteen siblings in `poller.rs` all have this shape:

  ```rust
  loop {
      if let Some(x) = retry::run(&policy, || fetch(&client)).await { write(x); }
      tokio::time::sleep(policy.budget).await;   // budget == that source's interval
  }
  ```

  The sleep is the whole gap between polls, not the period. There is no `tokio::time::interval` and no
  `MissedTickBehavior` anywhere in the file. So the real period is `interval + fetch`, the achievable
  rate is `3600 / (interval + fetch)` per hour, and the shortfall against a target of
  `3600 / interval` is `fetch / (interval + fetch)` of it. The error is a fixed proportion of the
  period, so a short interval is hurt most.

  **Seventeen pollers share the shape**, from `PollerConfig::intervals`, which returns 17 entries.

  `poller-check.sh:268` is the target: `expected_per_window()` is `WINDOW_SECS / interval`, read from
  the backend's own boot line. It has never accounted for the fetch.

  **Measured, not assumed.** The spacing between consecutive poll log lines for one source is
  `interval + fetch`, because the loop logs once per completed poll and then sleeps. That measurement
  was already in the record and the first version of this finding did not use it. Over 20 h of
  production on 2026-10-04, 15,249 gaps:

  | source | interval | p50 period | p99 period | implied fetch (p50) | achievable vs target | steady loss |
  |---|---|---|---|---|---|---|
  | xray | 120 s | 120.059 s | 120.224 s | 0.059 s | 29.99 of 30 | 0.05% |
  | kp | 60 s | 60.048 s | 60.188 s | 0.048 s | 59.95 of 60 | 0.08% |
  | imf | 60 s | 60.086 s | 60.297 s | 0.086 s | 59.91 of 60 | 0.14% |
  | solar-wind | 60 s | 60.108 s | 60.340 s | 0.108 s | 59.89 of 60 | 0.18% |
  | iss | 5 s | 5.027 s | 6.279 s | 0.027 s | 716.1 of 720 | 0.54% |

  So the real steady-state loss is **0.05% to 0.54%**, not the 0.8% to 3.2% first recorded here. Zero
  of the 15,249 gaps exceeded 1.5 times their interval. Nothing in that range reaches
  `THROUGHPUT_SOFT` of 90, and `THROUGHPUT_MIN_MISSED` of 10 would stop it mailing even if it did. At
  the 30 s ISS interval the range narrows to 0.05% to 0.18%.

  **The first version of this entry inverted the formula instead of reading the recorded period**, and
  assumed a 1 s fetch where the measured median is 0.027 to 0.108 s. Every figure in that table was
  10 to 30 times too large. Corrected 2026-10-04.

  **The retry arithmetic is the part that is not negligible.** `retry::Policy::new` sets
  `attempt_timeout = min(HTTP_TIMEOUT, max(interval, 2 s))` and `budget = interval`, with
  `BACKOFF_BASE` 250 ms doubling. A retry to failure therefore consumes the whole budget and is then
  followed by a full sleep, so the period **doubles**. For xray that is 120.25 s of retrying plus
  120 s of sleeping, about 240 s. One timeout then a success is about 1.5 times the period.

  Visible in the live record rather than inferred: `poller/iss` at a 5 s interval has a maximum
  observed period of **10.007 s**, exactly its own doubled period, and logged **23 ERROR lines in
  20 h**, each one a retry to failure. That is about 1.15 lost polls an hour out of 720, or 0.16%,
  so even this is small at a healthy rate. The other errors in that window were 6 on `poller/apod`,
  which also had the only 4 retried successes; `poller/apod` runs on a 3600 s interval, so the
  10.007 s periods are not and cannot be its.

  **The 557 to 626 ISS episodes had two causes, not one.** An earlier report of mine said no upstream
  failure was required to produce them. That was wrong: at the measured 0.027 s median, 557 of 720
  requires a fetch of about 1.46 s, a 54-fold increase, so the upstream did slow. What this finding
  adds is the amplification. The same 1.46 s latency costs 23% of delivery at a 5 s interval and about
  5% at 30 s, because the loss is `fetch / (interval + fetch)`.

  So **lowering `ISS_INTERVAL` to 30 s is right for the amplification, not for an unreachable
  target.** The target was always reachable to within half a percent. What the short interval did was
  turn an ordinary upstream latency excursion into an alert.

  Not fixed. A fixed-period tick changes behaviour rather than only timing: `MissedTickBehavior`
  decides what happens after a slow period, and the default fires the missed ticks back to back, which
  would send a burst at an upstream that was just slow. Seventeen call sites and a burst policy to
  choose, so it is its own change. Given the measured magnitude it is also not urgent.

  Recorded 2026-10-04 while lowering `ISS_INTERVAL` to 30 s. Corrected the same day against measured
  data. Naming the cause of a single bad hour still needs per-request duration recorded durably, which
  is the open item in the `## Measurement` section above.

- **AUD-044** A 25.1 hour outage on 2026-07-21 lost 1,505 minutes of X-ray flux permanently, and
  nothing in the repository recorded it until now.

  `xray` has a gap from `2026-07-21T18:27:00Z` to `2026-07-22T19:33:00Z`, 90,360 seconds. Measured
  from the stored series on the production database: 128,706 distinct observations from 2026-07-06,
  exactly two rows each for the two energy bands.

  **It is ours, and the one-day window is why it is permanent.** `fetch_xray` reads
  `services.swpc.noaa.gov/json/goes/primary/xrays-1-day.json`, a rolling one-day file. Every poll
  downloads the whole day and the writer keeps what is new, so an outage shorter than the window
  backfills itself completely on recovery. An outage **longer** than the window cannot: by the time
  the poller returns, the rows from the start of the gap have already left the file. 25.1 hours is 1.1
  hours past the horizon, and those 1,505 minutes are unrecoverable from this source.

  That makes the window a hard deadline on recovery time for every source read this way, which is a
  property no alert currently knows about. It is not the same thing as a freshness threshold:
  `SERIES_FRESHNESS` for xray is 300 s and would have fired within minutes, so the outage was
  detectable. What was missing is that the cost of not recovering changes discontinuously at 24 hours,
  and nothing says so.

  **The largest single data loss in the record.** The full distribution of gaps over 60 s is 36 gaps
  and 1,726 unstored minutes, so this one event is 87% of all X-ray data ever missed. Second is
  4,920 s on 2026-09-10, 81 minutes. Four gaps of 30, 26, 21 and 18 minutes cluster on 2026-09-21 and
  22. The remaining 21 are single minutes, 0.016% of the record, and are the upstream rather than us.

  The feed itself is not the problem: **99.97% of the 128,669 intervals are exactly 60 s.**

  Which sources share the exposure has not been enumerated, and that is the next step rather than part
  of this entry. The question for each is the width of the upstream window against the longest
  plausible outage. `xray` reads a one-day file. Not recorded here are the windows for the other NOAA
  products, which are a mix of rolling files and full products, so the check is per endpoint.

  No cause for the outage itself is recorded. 2026-07-21 predates the earliest logs still on the host,
  and nothing in the repository mentions that date.

  Recorded 2026-10-04, found while measuring X-ray observation spacing to look for a second cause
  behind a 90% delivery hour. The 90% turned out to be three missed polls at a low expected count,
  which `poller-check.sh:67` already explains, and this was the real finding in that data.

- **AUD-045** The email lowercase migration folds the key and follows none of its references.
  `db.rs:1279`, `migrate`.

  ```sql
  UPDATE users SET email = lower(email) WHERE email <> lower(email)
  ```

  `users.email` is the primary key and six tables reference it by `user_email`: `api_keys`
  (`db.rs:227`), `usage_records` (`237`), `webhooks` (`245`), `email_alerts` (`255`),
  `custom_anomaly_rules` (`265`) and `alerts_anomaly` (`185`). Folding the key without folding the
  references orphans every row that pointed at the mixed-case spelling. An orphaned `api_keys` row is
  a key that authenticates nobody; an orphaned `webhooks` row is a delivery that never fires again.

  **It has never triggered.** Verified on production 2026-09-23: zero addresses carried upper case and
  zero orphans existed across all six tables. The defect is the pattern, not the damage. It is
  recorded because the next migration written to this shape may touch a column that does have variance,
  and because the surviving code teaches the shape.

  **Decided 2026-10-04: delete the migration rather than complete it.** It has never folded anything
  on production, so the reference-following version would be new code on a path that has never been
  taken, and untaken code is where defects live unobserved. Deleting it removes both the orphaning
  shape and the obligation to maintain a fold nobody needs.

  Recorded here so nobody restores a half version later: **the danger is a future edit that reinstates
  the `UPDATE users SET email = lower(email)` without the reference fold**, which is the state this
  entry describes. If address case ever does need normalising, it is a new migration written with the
  six references in the same transaction, not a revival of this one.

  **AUD-045 closed 2026-10-05**. Deleted, with the constant, and the reasoning left in place where the next
  person will look for it: the six referencing tables by name, the production measurement, the four
  call sites of `auth::normalise_email` that make a new mixed-case row impossible, and an explicit
  instruction not to revive an `UPDATE users SET email = lower(email)` without the reference fold.

  The schema comparison under AUD-048 is what proves the deletion left a fresh install and a migrated
  one in the same shape, which is why the two were one change.

- **AUD-046** A webhook whose `events` column fails to parse silently matches nothing and logs nothing.
  Two sites, not one: `db.rs:4477` in `list_active_webhooks_for_event` and `db.rs:4434` in
  `list_webhooks`.

  ```rust
  let events: Vec<String> = serde_json::from_str(&events_json).unwrap_or_default();
  if events.iter().any(|e| e == event_type) {
  ```

  An empty vector matches no event type, so the row is filtered out of the delivery set and the
  customer's webhook stops firing with no error anywhere. The second site is worse in a different way:
  `list_webhooks` is what the customer's own dashboard reads, so the row renders with an empty event
  list and the UI agrees with the silence.

  **The clearest case in the codebase of `unwrap_or_default` swallowing a real error**, and the
  standing rule against ignored errors names exactly this. The two sites were found by scanning
  `db.rs` for `from_str` followed by `unwrap_or_default` rather than from the report, which named one.

  **Decided 2026-10-04: stay closed and stop being silent.** A row whose filter will not parse
  delivers nothing, which is what it does today, because delivering everything would push data to a
  customer who never asked for it. What changes is that the failure becomes visible in three places
  rather than none.

  Closed by all three, since any one alone leaves somebody blind:
  - an `error!` at both sites carrying the webhook id and `user_email`, so the operator sees it
  - the same condition surfaced to the owner through `list_webhooks`, which is `db.rs:4434`, the
    second site. Their dashboard currently renders the row with an empty event list and agrees with
    the silence, so it is the one place the person who can fix it would look
  - a fixture row holding invalid JSON in `events`, asserting the condition is reported rather than
    absent, at both sites

  Fail-closed delivery, loudly.

  **AUD-046 closed 2026-10-05**, commits `afe5b3e` for the fix and `beb4351` for the guard, split because
  stopping recurrence is separate work from the fix.

  `parse_event_filter` replaces both `unwrap_or_default` calls and is the only place the column is
  read. On a parse failure it logs an `error!` carrying the webhook id, the owner and the first 120
  characters of the stored value, and returns an empty filter paired with a flag.

  **Neither SELECT read `user_email`, so the owner was not available to log.** That is why the finding
  said two sites and the fix touched three things: `list_active_webhooks_for_event` now selects it as a
  sixth column. The report named the silence and not the missing column, which only appeared once
  there was a log line that needed an owner to name.

  The two sites move in opposite directions on purpose. Delivery drops the row, unchanged, because
  delivering every event to a customer who asked for none is the worse failure. The dashboard does the
  opposite and still returns the row, flagged through `WebhookRow::events_malformed` and surfaced in
  the webhook JSON, since hiding it from the only person who can repair it is the original defect in
  different clothes.

  The flag is carried rather than inferred from `events` being empty. An empty filter is a deliberate
  subscription to nothing and is not a fault, so the two states have to stay distinguishable.

  **Five mutations, all caught, and each by a different assertion:**

  | mutation | killed by |
  |---|---|
  | both sites swallow the parse again | the owner sees no flag: `[]` against `["notjson", "wrongshape"]` |
  | delivery stops failing closed | the delivery set grows to all three rows |
  | the dashboard hides the broken rows | the owner sees 1 webhook of 3 |
  | an empty filter is flagged as malformed | the empty-filter row is reported as a fault |
  | a seeded row goes missing | the floor: the seed did not land |

  **That the five died on five separate assertions is the part worth keeping.** The swallow mutation
  was killed by the dashboard flag and not by the delivery list, which is the correct outcome and the
  evidence the two halves are independently guarded: swallowing leaves delivery behaviour identical,
  so only the flag can observe it. Had the delivery assertion answered for it, one assertion would
  have been covering both halves and a later change to either would have gone unguarded.

  **Three of the five mutations were wrong before they ran**, recorded because the harness is now
  trusted with verdicts. One would not have compiled, so under the `ran == 0` rule adopted earlier
  today it would have read as NOT EXERCISED rather than as a pass. One broke the seed's SQL instead of
  the floor, which panics in the seed's own `expect` and is caught whether the floor exists or not: a
  mutant killed by the wrong mechanism looks identical to one killed by the right one. One anchored
  text the formatter had since joined onto a single line, and reported ANCHOR MISS. Only the first two
  were caught by reading; the third needed the run.

- **AUD-047** `unwrap()` in production code. `routes.rs:1606`.

  ```rust
  let tools: serde_json::Value = serde_json::from_str(MCP_TOOLS).unwrap();
  ```

  Inside `mcp_handler`, on the `tools/list` path. The input is a `const &str` in the same file and
  `the_server_card_advertises_what_the_endpoint_serves` plus two sibling tests parse it, so it cannot
  fail in practice. The rule is no `unwrap` outside tests, with no practical-impossibility exemption,
  and `routes.rs:2669` already uses `.expect("MCP_TOOLS is json")` for the same parse in a test.

  **AUD-047 closed 2026-10-04**, commit `8051082` for the fix and `f0c22e5` for the guard. Marked on
  2026-10-05 after verifying it in the code rather than in this entry: `MCP_TOOLS_PARSED` is a
  `OnceLock` filled by `init_mcp_tools()` before the database opens, the handler answers `-32603` when
  it is empty, and the only `from_str(MCP_TOOLS)` outside that function is in a test. The entry carried
  its plan wording alone while the work was already done, which is the state this file exists to
  prevent.

  Closed by parsing once into a `LazyLock<serde_json::Value>` so the cost and the failure both move to
  startup, or by returning a 500 through the existing `AppError`. The first is better: a manifest that
  does not parse should stop the process, not serve one broken route.

- **AUD-048** The `custom_anomaly_rules` declaration contradicts the live table and the no-float rule,
  and a fresh install is correct only by migration order. `db.rs:271`.

  ```sql
  threshold  DOUBLE  NOT NULL,
  ```

  No `threshold_scaled` column is declared. The live shape comes from a later migration that adds the
  scaled column and drops this one, so `CREATE TABLE IF NOT EXISTS` plus that migration happen to
  converge. Two things are wrong with that. The declaration states a float where the standing rule
  requires scaled integers, so the file teaches the wrong thing to the next reader. And correctness
  depends on the migration running after the DDL on every path, which nothing asserts.

  **AUD-048 closed 2026-10-05**. The DDL declares `threshold_scaled BIGINT`, nullable rather than NOT NULL so
  a fresh database matches what the `ALTER` produces on an older one, and the `RULE_THRESHOLD` backfill
  is guarded by `needs_threshold_backfill`, which counts the column in `duckdb_columns()` and skips the
  select when it is absent. `a_new_database_and_a_migrated_one_agree_on_every_column` compares both
  schemas in full, with a floor of 100 columns so an empty read cannot pass vacuously.

  **The fix broke every new install for an hour, and the fixture caught it on its first run.**
  Declaring the real shape removed `threshold`, while the backfill still ran
  `SELECT id, metric, threshold ... WHERE threshold IS NOT NULL`. On a fresh database that cannot bind,
  so `Store::open` failed with a DuckDB binder error before the listener bound. Half the decision had
  been applied, the declaration, and not the other half, the no-op.

  **The comparison alone was not enough, and that is the finding worth keeping.** Restoring
  `threshold DOUBLE NOT NULL` to the DDL left the two schemas still agreeing, because a fresh database
  then takes the same ALTER-then-DROP path a migrated one takes, and that mutation survived. A
  consistency check between two artifacts cannot detect both being wrong in the same way. So
  `the_ddl_declares_the_column_the_inserts_write` reads the `SCHEMA` constant directly and asserts the
  declaration, with its own floor against a mis-bounded slice.

  **Third instance of that shape in one day.** `the_server_card_advertises_what_the_endpoint_serves`
  compared `MCP_TOOLS` against the published card as `(name, description)` pairs and held both
  identically wrong at "3-hour" for as long as the model had been multi-horizon. The User-Agent
  behavioural test could not see a reverted per-request `unwrap`, because the response was identical.
  And this one. In each case the repair is the same: derive the expectation from the authority, or
  assert the source, rather than comparing two copies of it.

  Four mutations, all caught: the DDL regression by the source guard, the missing backfill guard and an
  emptied schema read by the comparison, and a guard that defaults instead of propagating by the
  AUD-049 rule.

  One pre-existing defect surfaced by the deletion, recorded because nothing could have found it
  otherwise: `TOKEN_VERSION_MIGRATION` was undocumented while the comment describing it sat above
  `EMAIL_LOWERCASE_MIGRATION`, which had been inserted between a doc comment and its constant. A doc
  comment on the wrong item is still a valid doc comment, so no lint saw it until its neighbour went
  away.

- **AUD-049** A migration check that fails open. `db.rs:1301`.

  ```rust
  let needs_forecast_rekey: i64 = conn
      .query_row("SELECT COUNT(*) FROM duckdb_columns() WHERE table_name = 'kp_forecast'
                  AND column_name = 'horizon_hours'", [], |row| row.get(0))
      .unwrap_or(1);
  if needs_forecast_rekey == 0 { ...rekey... }
  ```

  A failing schema query returns 1, which reads as "the column is already there" and skips the rekey.
  The fail-closed rule says authorization, integrity, secret loading, environment selection and
  migration all close to the safe state when uncertain, and the safe state for a migration check is to
  attempt the migration or refuse to start, never to assume it is done.

  **The live exposure is narrow and specific.** The rekey has already run in production, so a failing
  query there changes nothing today. The exposure is a restore from a backup predating the rekey: the
  query fails, the rekey is skipped, and the service runs on the old key shape with no complaint.

  **AUD-049 closed 2026-10-04**, commit `8051082`, guarded by `f0c22e5`. Marked on 2026-10-05 after
  verifying it in the code: `db.rs` carries no `unwrap_or(1)` outside the comments that quote the old
  line, and `every_migration_decision_propagates_its_query_error` is the rule test over the decision
  bindings.

  Closed by propagating the error with `?` so a failing check stops startup. One line, and the test is
  a fixture whose schema query errors, asserting the open fails rather than proceeding.

- **AUD-050** CORS allows any origin on every route, and the premise that made that safe has changed.
  `routes.rs:563`.

  ```rust
  CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
  ```

  Applied to the whole router, including `/auth/*` and `/mcp`. The reason this was not a CSRF path is
  that authentication is a Bearer token in a header, which a cross-origin page cannot make the browser
  attach, and `allow_credentials` is not set, so cookies are not sent either. That still holds for the
  session token.

  **What changed is that the OAuth state now lives in a cookie.** The browser-binding work put it
  there, so there is now a cookie in the authentication flow where previously there was none. Whether
  that is reachable through this layer depends on `allow_credentials` staying unset and on the state
  cookie's own `SameSite`, neither of which is asserted anywhere.

  **Decided 2026-10-04: the split layer.** Any-origin stays on the data routes, because browser
  callers of the public API are a product promise and `/api/public/*` and `/mcp` are deliberately
  open. The credential routes get an origin-restricted layer, with a test that a cross-origin
  preflight to `/auth/login` is refused.

  **The `/auth` prefix is not the boundary, and that is the part worth recording.** Routes outside it
  create and destroy credentials: `/api/keys` and `/api/keys/{id}` mint and revoke API keys, and
  `/api/user/plan` changes billing state. A split written on the prefix would leave all three on the
  open side while looking complete, which is the same shape as a check that enumerates from the
  protection instead of from the asset.

  So the side a route lands on is decided by **what it does, not where it sits**. The rule, to be
  applied by enumerating every `.route(...)` in `routes.rs` rather than by matching a path:

  > A route is credential-bearing if it creates, reveals, changes or revokes a means of
  > authentication, or changes billing or plan state. Everything else is data.

  Stated so a route added later lands on the right side without re-deriving the argument.

  One thing this entry should not overclaim: with `allow_origin(Any)` and `allow_credentials` unset,
  a cross-origin page cannot attach the session Bearer token nor the OAuth state cookie, so there is
  no live bypass today. The split is defence in depth, and specifically it is what stops a future
  `allow_credentials(true)` from turning into a vulnerability with no other edit. Closing it means the
  split, the enumerated list recorded, and the preflight test.

  **AUD-050 closed 2026-10-05**, commits `89a66a9` for the split and `316e6d1` for the guard.

  **The rule that shipped is narrower than the one written above, and the reasoning is the part worth
  keeping.** Two axes were measured over all 56 routes.

  Axis A, does the route require a credential, is mechanical: `AuthClaims` is a `FromRequestParts`
  extractor, so a handler requires one exactly when its signature names it. Forty of 56 do. The
  control on that number is that the other 16 are exactly the seven public routes, the eight
  unauthenticated auth endpoints and `/mcp`, which authenticates per tool inside the handler.

  **Axis A is the wrong axis.** Thirty-four of those 40 are `/api/kp`, `/api/neo` and `/api/reports/*`.
  The product is an API that third parties call, including from browsers with their own key, so an
  origin allow-list there breaks the thing being sold rather than protecting it.

  **CORS also protects nothing on a route that already requires a bearer token.** A page that does not
  hold the caller's token cannot make an authenticated request whatever the origin policy says. What
  the browser's origin check can protect is the narrow set that mints or accepts a credential with none
  already present, because there the origin is the only check available. Six routes.

  So the rule as shipped:

  > A route goes on the strict layer when a password, a session token, a single-use email token, a TOTP
  > secret or generated key material crosses the wire in either direction. Everything else keeps the
  > wildcard, because it is an API for third parties to call.

  **This supersedes the rule written above on one route.** `/api/user/plan` stays permissive. A plan
  change is not a means of authentication, and the route already requires a token, so an allow-list
  over it protects nothing. Billing state belongs to the authorization rules rather than to CORS.

  The strict layer, 16 routes:

  - **Real protection**, reachable with no credential, so the origin is the only check:
    `/auth/register`, `/auth/login`, `/auth/2fa/login`, `/auth/forgot-password`,
    `/auth/reset-password`, `/auth/verify-email/{token}`
  - **Defence in depth** at no cost, since a token is already required: `/auth/change-password`,
    `/auth/resend-verification`, `/auth/2fa/setup` which returns the TOTP secret, `/auth/2fa/verify`,
    `/auth/2fa/disable`, `/api/keys` whose POST returns the raw key once, `/api/keys/{id}`,
    `/api/webhooks` whose POST returns the signing secret
  - **CORS does not reach them at all**: `/auth/oauth/{provider}/start` and
    `/auth/oauth/{provider}/callback` are top-level navigations returning 302. Listed so the next
    reader does not re-derive it.

  The remaining 40 keep the wildcard, unchanged.

  **`ALLOWED_ORIGINS`, comma separated, deliberately separate from `APP_URL`.** `APP_URL` means where
  the frontend lives and is read for email links. Giving it a second meaning is the drift this project
  has been removing. Production may need `astraeusio.com` and `www.astraeusio.com` both.

  **Empty or unset closes rather than opens.** No origin matches, so every cross-origin request to a
  credential route is refused. That costs the application nothing: nginx serves the dashboard and
  proxies `/api/` and `/auth/` from the same origin, so the dashboard has never made a cross-origin
  request. `an_empty_origin_list_refuses_every_cross_origin_credential_request` asserts it rather
  than leaving it as a claim.

  **Decided 2026-10-05: production ships with no configured origins.** `ALLOWED_ORIGINS` is set
  nowhere, in `backend/.env` or in `docker-compose.yml`, and that is the decision rather than an
  omission. Nothing makes a cross-origin credential request today, so a configured list would be work
  for a case that does not exist. The first consumer that actually needs one is what makes the list
  worth writing.

  To enable it later, either of two places, not both:

  - `docker-compose.yml`, the `backend` service's `environment` block, beside the `DB_PATH`,
    `ML_SERVICE_URL` and `BIND_ADDR` overrides already there. Tracked, so it deploys with the repo and
    is visible in review
  - `/opt/astraeusio/backend/.env` on the server, which that service loads through `env_file`.
    Untracked, so it stays out of the repository and has to be set again on a fresh host

  The value is comma separated and each entry carries its scheme, for example
  `https://astraeusio.com,https://www.astraeusio.com`. A bare host is dropped with an `error!`, and a
  `*` is refused, so a wrong value narrows access rather than widening it. Startup logs the count when
  the list parses and warns when it is empty, which is how to tell the two apart in the container logs.

  **A consequence accepted, not overlooked.** A CORS layer attaches per path and not per method, so
  `GET /api/keys` and `GET /api/webhooks` ride into the strict layer with their POSTs. A third party
  cannot list their own keys or webhooks cross-origin from a browser. A deliberate loss.

  **The split introduced a regression, and measuring is what found it.** Under one layer the fallback
  sat inside it, so an unmatched path answered 404 carrying the wildcard. After the split an unmatched
  path matched neither sub-router and answered a CORS failure instead, turning a third party's mistyped
  path into a CORS error rather than a 404. `.fallback(not_found)` inside the permissive layer restores
  the old answer, with the same empty 404 body. Reasoning had not raised it; a probe did.

  **One redundancy the mutation round exposed.** `parse_allowed_origins` refuses a literal `*` and also
  refuses anything without a scheme, and neither check is individually observable, because `*` carries
  no scheme and the scheme check drops it on its own. Disabling either alone leaves behaviour
  unchanged. Both are kept, the wildcard branch for its clearer message, and the mutation removes both
  together since that is what the property actually rests on.

  **Six mutations, each caught by the test named for it:** the whole router tightened instead of split,
  an empty list falling back to permissive, a credential route moved to the permissive layer, the
  permissive layer applied after the merge instead of before, nothing stopping a wildcard reaching the
  layer, and the fallback outside both layers. The third was moved rather than deleted on purpose:
  deleting `/auth/login` answers 404 with no origin header, which the first leg would read as a refusal
  and pass for the wrong reason.

  The harness now declares which test should kill each mutation and reports WRONG GUARD when a
  different one does, because a mutant killed by another layer says nothing about the layer under test.

  **Three method errors of mine, recorded because this entry's own rule is to enumerate from the
  asset.** A keyword scan called `/api/anomalies` credential-bearing, since its handler names `Claims`
  to check auth, which is most authed routes, and it missed four others. A flat function-name index
  then collided: `list_webhooks` and `get_user_me` exist both as route handlers and as `Store` methods
  in `db.rs`, so eight plainly authed routes read as unauthenticated until the lookup resolved the
  module. And I reported that `GET /api/webhooks` returns the signing secret, when the SELECT reads the
  column and the response omits it.

  **The worry that opened this entry is now fail-fast.** `allow_credentials` stays unset, and
  tower-http asserts that credentials cannot combine with any wildcard in `Layer::layer`
  (`cors/mod.rs:783`, called from line 494), which runs at router build. A later
  `allow_credentials(true)` on the permissive layer panics at startup instead of shipping quietly.
  Verified in the vendored source of 0.6.8, not from memory.

- **AUD-051** 58 dash-like characters remain in tracked files. The rule is none.

  Counted 2026-10-04 across all 176 readable tracked text files, 17 further tracked files being
  binary. **It was 61 before the blog corrections, so 58 is the baseline for the next count.**

  | class | count |
  |---|---|
  | en dash U+2013 | 36 |
  | minus sign U+2212 | 17 |
  | em dash U+2014 | 5 |

  Nineteen files carry them. `frontend/src/blog/posts.js` has 31 of the 58. Then `ml/preprocess.py` 5,
  `get-space-weather/SKILL.md` 3, `ml/download_kp.py` 3, `DocsPage.jsx` 2, and twelve files with one
  each.

  **Two things the raw count hides.** The 17 U+2212 are a different character from the two the rule
  names, so whether they are in scope is a decision rather than a defect. And almost every en dash is
  a numeric range, two of them in text a user reads: `routes.rs:1363` is API error copy, "name must be
  1-80 characters", and `AsteroidTable.jsx:104` renders a diameter range in the asteroid table. The
  rest are code comments and docstrings, plus one em dash in an HTML comment inside
  `astraeusio-logo.svg`.

  **Decided 2026-10-04: U+2212 is in scope. The target is 58, not 41.** A reader cannot tell a minus
  sign from an en dash on screen, so the rule is no dash-like character rather than no em or en dash.
  The gate step says this in its own text rather than leaving the next person to infer which of the
  three classes it counts.

  Closed by that gate step, counting all three classes across tracked text files and failing above
  zero, in the shape of `scripts/lib/naming.sh` with its own self test, so the count cannot drift back
  up unnoticed. Fixing the 58 without the step buys one clean day. The step is the deliverable; the
  sweep is its first run.

  Two of the 58 are text a user reads and need a replacement chosen rather than deleted:
  `routes.rs:1363` is API error copy, "name must be 1-80 characters", and `AsteroidTable.jsx:104`
  renders a diameter range. The rest are code comments, docstrings, and one em dash in an HTML comment
  inside `astraeusio-logo.svg`.

  **AUD-051 closed 2026-10-05**, commit `1b0ae08`. The sweep rides inside the commit that adds the step rather
  than taking a subject of its own, because a commit subject describing a dash cleanup is not one this
  project makes.

  `scripts/lib/dashes.sh` counts the three classes across tracked text files and fails above zero, in
  the shape of `naming.sh` with its own self test, wired as two gate steps. The step is named
  `dash sweep: U+2013 U+2212 U+2014, target 0`, so which characters it counts is in the gate output
  rather than inferred from the script. Seventeen steps now, from fifteen.

  The sweep: 58 characters across 19 files, every one a numeric range or a negative value, so a
  hyphen-minus is the replacement throughout. Three decided rather than substituted:

  - `ml/test_published_claims.py` searches published documents for the stale string `7` plus an en dash
    plus `48`. Replacing the character would have deleted the spelling the test exists to catch, so the
    source now carries it as a `\u2013` escape. The file is ASCII and the assertion is unchanged.
  - four SVG comments used an em dash as an apposition separator, which became a comma
  - `gen-blog-og.py` printed `Done` and an em dash before a count, which became a colon

  **The first version of the detector passed on a tree holding all 58, and its self test passed
  alongside it.** It assembled each pattern as a backslash-u escape around a variable. Measured in this
  bash, that form yields the six ASCII bytes `5c7532303133` rather than the character, while the same
  escape written as a literal, or taken whole from a variable, expands correctly. So the check searched
  for a string no file contains. The self test agreed with it because it planted the identical wrong
  string: a self test that plants what the detector looks for cannot see the two being wrong together.

  **That is the fifth instance of the shape today** and the first where it hid a real defect rather than
  merely failing to find one. The repair is the same as the others: the patterns are tied to each code
  point's UTF-8 encoding, which is a fact outside the file, and the self test checks the built bytes
  against it.

  **The step also had to be shown able to fail, not only to pass.** On a swept tree it passes, and a
  check that has never been seen to fail is indistinguishable from one that cannot. The failure path is
  controlled inside the self test over a planted file list rather than the working repository: a list
  holding a planted dash must be rejected, a clean list must not be, and an empty list must be rejected
  because a count of zero over nothing proves nothing.

  **Six mutations, each caught by the self test and by the message aimed at it:** a class dropping out
  of the list, the build reverting to an assembled escape, the per-file count always answering zero, the
  gate passing despite a non-zero count, the empty-list floor removed, and the binary test inverted so
  every text file is skipped. The control step is therefore what guards the gate step, which is the
  claim worth having rather than six green lines.

  **Two things the sweep turned up that reasoning had not.** `dashes.sh` itself carried two en dashes in
  its own header, and the gate passed because `git ls-files` does not see an untracked file: it would
  have rejected itself the moment it was staged. And `get-space-weather/SKILL.md` has a published sha256
  in `agent-skills/index.json`, so editing it broke the `skill hashes` step until the manifest was
  regenerated from the file.

  **One thing this did not fix.** The standing rules say the pre-commit hook checks dashes. It does
  not. The
  hook at `core.hooksPath` runs the naming rule and then delegates to a repository hook that this
  repository does not ship, so the rule is enforced in the gate and nowhere else. Recorded rather than
  widened into this change.

  **AUD-053 closed 2026-11-24**, commit `6a96f6f` for the fix and `3b2c1ad` for the guard.

  Fixed at the ordering rather than at the comparison. `Store::open` takes the secret as an argument,
  `Store` keeps it so `try_clone` reuses the value it was opened with, neither site reads the
  environment, and `main` validates before opening anything through `require_jwt_secret`, which
  returns an error instead of panicking and refuses a blank value as well as a missing one.

  This is the shape `main.rs`'s own header comment had already prescribed for the eleven environment
  reads behind its unsafe-in-tests exemption: the logic takes the value rather than reading the
  process. One of the eleven is now gone.

  **Four tests.** Two behavioural, in `db.rs`: equal keys refuse to open, distinct keys open, and the
  clone keeps comparing against the value it was given. Two scans: `db.rs` reads `JWT_SECRET` from the
  environment nowhere, and `main` validates before it opens, asserted on source order because the
  ordering is the defect and a running process cannot show it.

  **The first version of the behavioural test could not tell the two apart, and the mutation round is
  what found that.** It left `JWT_SECRET` unset, so a site that still read the environment compared
  the TOTP key against an empty string, never matched, and opened successfully. Both `is_ok`
  assertions held whether the value came from the argument or from the process, and the `try_clone`
  mutation was caught only by the source scan, which the harness reported as WRONG GUARD rather than
  as a pass. The test now sets `JWT_SECRET` equal to the TOTP key on purpose: any site reading the
  environment then refuses, so the two assertions discriminate.

  **Six mutations, each caught by the test aimed at it:** each of the two sites reading the
  environment again, a blank secret accepted again, the validation moved back below the open, the
  no-environment scan reading nothing, and the ordering scan no longer stripping comments.

  **An unrelated defect this uncovered, and it had to be fixed for the new test to mean anything.**
  Seven sites across `db.rs` and `secretbox.rs` mutate `TOTP_ENCRYPTION_KEY`, each carrying the
  comment "SAFETY: single threaded test". None of them was: `cargo test` runs tests on several
  threads inside one process, so three tests were already racing over one process-global variable and
  the comments asserted the opposite. `secretbox::ENV_LOCK` now serialises them, and the pre-existing
  sites take it too, because a lock only works if every participant does. Poisoning is recovered from
  rather than propagated, so one panicking test does not fail the others with an unrelated message.

  **Both source scans read their own test code first.** The `main` scan counted the fixture that saves
  and restores `JWT_SECRET` as a second production reader, and after fixing that, the `db.rs` scan did
  the same thing for the same reason. Both now slice at the test module, anchored on
  `mod tests {` rather than on the attribute, because `main.rs`'s header comment quotes
  `#[cfg(test)]` while explaining the unsafe exemption and slicing there cut the file off at line 18.
  Three distinct ways for a scan to read the wrong text, in one change.

- **AUD-054** Five checks in one session compared two artifacts and could not see both being wrong
  the same way. The fifth hid a live defect rather than merely failing to find one.

  Recorded 2026-10-05 as a method finding. The individual repairs are already closed under their own
  identifiers; what is new here is the shape and its frequency, because a defect found is a pattern to
  hunt and this one recurred five times in a day.

  **The shape.** A check asserts that two things agree. Agreement is necessary and not sufficient: if
  both derive from the same mistake, or one was written by copying the other, the check passes while
  the property it stands for is false. The repair in every instance is the same, which is why it is
  worth writing once: derive the expectation from the authority, or assert the source directly, rather
  than comparing two copies of it.

  | # | the check | what it could not see | repair |
  |---|---|---|---|
  | 1 | `the_server_card_advertises_what_the_endpoint_serves` compared `MCP_TOOLS` against the published card as `(name, description)` pairs | both said "3-hour" for as long as the model had been multi-horizon | the expected phrase is rendered from `FORECAST_HORIZONS` |
  | 2 | a behavioural test that the shared HTTP client sends a User-Agent | a reverted per-request header, because the response was identical either way | assert the client construction, not only the response |
  | 3 | `a_new_database_and_a_migrated_one_agree_on_every_column` | a fresh database taking the same ALTER-then-DROP path as a migrated one, so both agreed while the DDL was wrong | `the_ddl_declares_the_column_the_inserts_write` reads the `SCHEMA` constant |
  | 4 | the AUD-046 mutation round, where disabling the wildcard branch of `parse_allowed_origins` changed nothing | two guards masking each other, since a wildcard has no scheme and the scheme check drops it alone | the mutation removes both, since that is what the property rests on |
  | 5 | `dashes.sh --self-test`, which planted a character and found it | the planted string and the search pattern were the same wrong six ASCII bytes | patterns tied to each code point's UTF-8 encoding, asserted against it |

  **The fifth is the one that cost something, and the measurement is the record.** The detector
  assembled each pattern as a backslash-u escape around a variable. Measured in this bash, inside a
  file so no shell layer could rewrite it:

  | form | bytes produced |
  |---|---|
  | the escape written as a literal | `e28093`, the character |
  | the format taken whole from a variable | `e28093`, the character |
  | the escape assembled around a variable | `5c7532303133`, the six ASCII bytes of the escape |
  | hex escapes through `printf %b`, as shipped | `e28093`, the character |

  So the gate step searched for a string no tracked file contains and reported a clean tree while 58
  dash-like characters sat in it. The self test passed beside it because it planted the identical
  `5c7532303133`. Both halves were wrong together and agreed, which is the whole shape in one file.

  An earlier draft of this record claimed the cause was that this bash does not expand a
  backslash-u escape. The table above is why that was corrected: two of the four forms expand
  correctly, and the one that fails is specifically the assembled one. A cause recorded from
  recollection rather than from measurement would have sent the next reader looking for the wrong
  thing.

  **What the five have in common beyond the shape.** Four were caught by mutation testing and one by
  an independent count. None was caught by the check itself, which is the argument for a positive
  control on every check rather than on the ones that feel risky: a check that has only ever been seen
  to pass is indistinguishable from one that cannot fail.

- **AUD-055** The git hooks enforce one rule, not the five the standing notes list. Found 2026-10-05
  while closing AUD-051, by reading the three hook files rather than the notes.

  `core.hooksPath` points outside the repository, so the hooks are not tracked here and a clone does
  not get them. Each hook runs the naming rule and then calls `naming_delegate`, which looks for
  `scripts/hooks/<hook>`, `.githooks/<hook>` and `hooks/<hook>`. This repository ships none of those,
  so the delegation is a no-op and the naming rule is the whole of it.

  | hook | what the notes claim | what it does |
  |---|---|---|
  | `pre-commit` | dashes, staging of the instruction files, formatter, banned names | the naming rule over staged paths, staged added lines and the git identity. The instruction files are covered, but only because their path carries the banned name. No dash check. No formatter. |
  | `commit-msg` | one line, conventional prefix, no dash, no trailers, no cleanup-only subjects | the naming rule over the message, plus a trailer check. One line, the prefix, the dash rule and the cleanup-only subject rule are not checked at all. |
  | `pre-push` | the naming rule across tracked content, the messages being pushed, and tag annotations | accurate. It also covers author and committer identity, changed paths and added content. |

  **So four commit-message rules this project follows are enforced by nothing.** One line, the
  conventional prefix, no dash in a subject, and no cleanup-only subject are habits, not gates. The
  standing notes say a rule that lives only in a document is one that can be forgotten, which is
  exactly their status.

  A fifth claim in the same place is also wrong in the gate's direction: the notes say the gate
  includes an unsafe-code attribute check. It does not. `main.rs:31` carries
  `#![cfg_attr(not(test), forbid(unsafe_code))]` and the compiler enforces it while it is there, but no
  gate step and no test asserts it is still there, so deleting that line compiles and nothing notices.
  The dash sweep in the same sentence became true with AUD-051.

  **Where the dash rule should live.** The gate is the right single place and it is now there. A
  repository-level `scripts/hooks/pre-commit` sourcing the same `dashes.sh` over staged added lines is
  worth adding on top, for one reason that is not tidiness: a dash caught at commit time avoids a
  commit that has to be amended, and this project prefers a new commit over an amend. The honest limit
  is that such a hook only runs where `core.hooksPath` already points at the delegating hook, so it is
  a convenience for this machine rather than enforcement. The gate stays the authority either way.
  Not built, because it is outside the step that found it.

- **AUD-056** The site was down for 16 h 46 m on 2026-10-27 because two services wanted ports 80 and
  443 and the winner was decided by boot order. Nothing in this repository could have caught it.

  Measured during the incident. `uptime -s` gave the boot at `2026-10-27T08:12:05Z`, the recreated
  frontend's `.State.StartedAt` gave `2026-10-28T00:58:21Z`, so the outage was 60,376 seconds.

  **The sequence, and the second cause is the one worth keeping.**

  1. The host rebooted for a kernel upgrade, `6.8.0-111` to `6.8.0-147` in `last -x reboot`. It was
     the first reboot in 170 days: the previous boot was 2026-05-10 and ran `170+06:59`.
  2. `caddy`, enabled since May and never able to bind while the container held the ports, started at
     `08:12:11` and took 80 and 443. Docker started backend and ml at `08:12:14`.
  3. The frontend container failed at the networking step. Its `.State.Error` read
     `failed to set up container networking: driver failed programming external connectivity ...
     failed to bind host port 0.0.0.0:80/tcp: address already in use`, exit 128. Because
     `restart: unless-stopped` treats a stopped container as deliberate, nothing retried it.
  4. Caddy answered 443 with a Let's Encrypt certificate that expired on `Aug 8 00:56:20 2026`, so
     Cloudflare under Full (strict) returned **526** for every request. Its renewals could never
     succeed: its own log says
     `open /var/lib/caddy/.../astraeusio.com/astraeusio.com.key: no such file or directory`. The real
     origin certificate was never involved and was valid throughout, `notAfter May 6 13:06:00 2041`.
  5. Disabling caddy cleared the certificate error and revealed the consequence of step 3. The
     container was left attached to **no network at all**: `.NetworkSettings.Networks` was empty while
     backend and ml sat on `astraeusio_default` at `172.18.0.2` and `172.18.0.4`. With no network there
     is no Docker DNS, so nginx aborted at config load with
     `host not found in upstream "backend" in /etc/nginx/conf.d/default.conf:89` and the edge returned
     **521** instead. The container crash-looped to 9 restarts.
  6. `docker compose up -d frontend` **starts** an existing container, so it reused the broken,
     network-less configuration. `docker compose up -d --force-recreate --no-deps frontend` rebuilt the
     endpoint, the container joined `astraeusio_default` at `172.18.0.3`, and the site returned 200.

  **A restart was the wrong instinct and would have failed.** It was the first assumption on both
  sides, and it re-runs the race in step 2, which caddy wins by three seconds.

  **Two failure shapes to carry forward.** A container whose start fails while binding a port is left
  with no network, and that failure then presents as a name resolution error in a later start rather
  than as the port conflict it actually is. And `up -d <service>` is not idempotent in the way it
  reads: it will happily start a container whose recorded configuration is broken.

  **What is enforced and what is not.** Nothing here. The gate reads this repository, and this was
  host state: a second web server installed outside the repository, enabled, holding the ports the
  compose file publishes. No tracked file could have expressed it. The honest mitigations are both
  outside the gate: remove caddy rather than leave it disabled, or give the host a boot-time check
  that the published ports belong to the containers that claim them.

  Caddy is disabled and still installed, with an expired certificate it cannot renew and a config
  pointing `reverse_proxy 127.0.0.1:8080` where nothing listens. Inert, and one `systemctl enable`
  away from doing this again.

  **The detection half is worse than the fault.** UptimeRobot polled throughout and its 200s resume in
  the access log at `01:00:49`, so the monitor saw the outage start at 08:12 and the owner learned of
  it 16 h 46 m later from a browser. Finding out why that alert did not arrive is worth more than
  either fix, and it is not a change to this repository.

- **AUD-052** Three counts in `Store::open` swallow a failing query, and each makes the code believe
  something different. None is the fail-open class AUD-049 was.

  Found by scanning `Store::open` for the shape AUD-049 had, rather than from a report. The function
  holds 29 `query_row` calls; 26 use `?` and these three do not. What decides whether each matters is
  what reads the value afterwards, not the `.unwrap_or` itself.

  | line | binding | what reads it | what a failing count makes the code believe |
  |---|---|---|---|
  | `db.rs:1418` | `expected` | the `if expected > 0` gate above `era_fix_is_verified` | **corrected 2026-11-24.** Not a false alarm. `expected` becomes 0, so the gate is false, the whole repair block is skipped and `era_fix_is_verified` is never reached. The `INSERT INTO schema_migrations` sits outside that gate, so the era fix is recorded as applied having done nothing, and nothing retries a migration that claims to be done |
  | `db.rs:1730` | `before` | `error!(before, after, "xray rebuild did not preserve every row")` at `db.rs:1752` | that the `xray` rebuild lost every row. `before` becomes 0 and the comparison against `after` reports a rebuild that destroyed data |
  | `db.rs:1691` | `carried` | one `info!` at `db.rs:1711` and nothing else | nothing. No decision reads it. The log line claims 0 rows were carried when rows were, so only the record is wrong |

  **That read was wrong on the first site, and the correction reverses the entry's conclusion.** This
  was written as "two fail loud and one is cosmetic, the opposite direction from AUD-049". Reading the
  control flow rather than the call site shows `expected` is the same class as AUD-049 and not its
  opposite: a swallowed error makes a migration skip its work and still record itself as done. The
  other two are a false alarm about data loss and a misleading log line. So one of the three is
  silent and permanent, which is the worst of the three, and the entry had it as the mildest.

  The mistake was reading what consumes the value and stopping there. `era_fix_is_verified` does
  consume `expected`, which is what the table said, but it sits inside a gate that a zero closes, so
  the consumer is never reached. Tracing to the consumer is not the same as tracing the control flow
  that reaches it.

  **AUD-052 closed 2026-11-24**, commit `c01039b` for the fix and `3b584e9` for the guard.

  All three now use `?`. The guard is a new test, `every_query_in_store_open_propagates_its_error`,
  because the existing one could not have caught them: it enumerates bindings named `needs_*` or
  `*_applied`, and these bind `expected`, `carried` and `before`. A rule written on a naming
  convention covers the names someone thought of. The new one enumerates the 28 `query_row` calls in
  `Store::open` instead, so a count added later is covered whatever it is called.

  It is scoped to statements holding `query_row(`, because two other `unwrap_or` calls in that
  function are not query results and must stay: the `JWT_SECRET` read, which AUD-053 moves out
  entirely, and `scale_threshold`, which deliberately keeps the nearest representable value rather
  than dropping a user's rule.

  **Six mutations, each caught by the message aimed at it:** each of the three sites swallowing again,
  a query discarded with `.ok()` instead of a default, the scan matching no query at all so every
  comparison below it would hold over an empty list, and the comment stripper removed so the comments
  this fix added, which quote `.unwrap_or(0)`, read as the defect they describe. That last one is the
  fifth time a scan in this repository has been at risk from its own documentation.

  **The hunt that found these was scoped to one file, and that is worth recording separately.** It
  scanned `Store::open` in `db.rs` and never left that function. Across shipped code there are 84
  `unwrap_or*` sites in 14 modules. Thirteen sit in a fallible-call position outside `db.rs` and none
  was looked at: `main.rs` 86, 97, 135, 143, 168; `poller.rs` 62, 71, 112, 117, 668; `routes.rs` 774;
  `mailer.rs` 18; `oauth.rs` 69.

  Read afterwards, all thirteen are environment or configuration defaults, the ones the project's
  local instruction file lists as optional with defaults in code, so no verdict changes. **The scope was wrong and the outcome was
  right**, which is luck rather than method: the one genuinely fail-open site happened to be inside the
  one function that was scanned. Had it been in `poller.rs`, nothing about the approach would have
  found it or reported that it had not looked.

  `routes.rs:906` was raised as a candidate and is not one. It defaults an absent `?page=` to 1 and
  clamps to at least 1, so a failing parse makes the code believe the caller asked for page 1, which is
  the documented behaviour and matches `parse_range` above it. User input failing to parse is the
  expected case, not a swallowed error.

  One inconsistency the sweep did turn up: `ML_TIMEOUT` is read with different defaults in two places,
  10 at `poller.rs:668` and 5 at `routes.rs:774`. One variable, two meanings depending on the path.

  **The wider hunt, 2026-11-24.** Enumerated from the source across every module rather than one
  function, which is what the scoping note above asked for. 36 shipped `.unwrap_or(0)` and
  `.unwrap_or_default()` sites, test modules excluded by brace depth from `#[cfg(test)]` rather than by
  name, because a name filter missed `mod mcp_tests` here once. A 37th match was this file's own doc
  comment quoting the pattern, which is why the count is 36.

  Real swallowed errors, none changed, each needing its own decision:

  | site | what a database error makes it believe | direction |
  |---|---|---|
  | `db.rs:3040` `get_token_version` | token version 0. The SQL already `COALESCE`s NULL, so this only catches a query failure or a missing row | fail **closed**: `auth.rs:1361` compares `claims.ver != current`, so a fault rejects every session carrying a non-zero version rather than accepting a stale one. Mass logout, no security hole, nothing logged |
  | `auth.rs:246` `current_token_version` | the same, through `.unwrap_or_default()` | the same |
  | `rate_limit.rs:279` | the same, and it caches the 0 in the usage counter, so the effect outlives the fault until the entry ages out | the same, but persistent |
  | `poller.rs:803` `list_enabled_email_alerts` | that nobody subscribed, so threshold emails stop | silent. No alert goes out and no line says why |

  Legitimate NULL or absent-value defaults, no change: `db.rs:3339`, `3342`, `3343`, `3344` and
  `db.rs:4871` all read `row.get::<_, Option<i64>>(n)?`, so the `?` carries any error and the
  `unwrap_or(0)` only handles a SQL NULL from a `COUNT(*) FILTER` or `AVG` over an empty group;
  `db.rs:3358` defaults an `Option` from `.find()`, and its comment says a horizon with no pairs must
  read as zero rather than vanish; `db.rs:4008`, `4011`, `4012` turn an absent value into an empty CSV
  cell; `routes.rs:1350` and `1365` default an absent billing period to zero after the query already
  used `?`.

  Clock defaults rather than query results, no change: `db.rs:1896`, `anomaly.rs:411`,
  `db_writer.rs:511`, `poller.rs:712`, `poller.rs:977`, `routes.rs:223`, `894`, `961` and `1765` all
  swallow `SystemTime::duration_since(UNIX_EPOCH)`, reachable only with a clock set before 1970, which
  would then write a timestamp of 0. `db.rs:3677` and `3680` turn an out-of-range timestamp into an
  empty date string, which the query would then match nothing against.

  **The two forms named are not the whole shape, which the scope should say.** Restricting to
  `.unwrap_or(0)` and `.unwrap_or_default()` misses the same defect spelled differently, and three
  sites in the first list above were found only because they sat next to one that matched:
  `poller.rs:801` and `802` use `.unwrap_or(None)` to turn a database error into "no reading", and
  `rate_limit.rs:281` uses `.unwrap_or_else(|_| "free".to_string())` to turn one into a plan
  downgrade. A sweep defined by spelling rather than by behaviour will keep finding only the
  spellings it was given. Widening that test to
  every `query_row` in `Store::open` is the obvious move and needs one judgement first: whether any
  count in that function is legitimately optional, because the test would then need an exemption list
  and an exemption list is the thing that rots.

  Not fixed. Recorded 2026-10-04 while closing AUD-049, so the reasoning for each is written while the
  call sites were in front of me rather than reconstructed later.

- **AUD-053** The `SameAsJwtSecret` guard cannot fire while `JWT_SECRET` is unset, and both callers
  that reach it run before `main` validates that variable.

  `secretbox.rs:62` refuses to start when the TOTP encryption key equals the JWT secret:

  ```rust
  if raw == jwt_secret {
      return Err(KeyError::SameAsJwtSecret);
  }
  ```

  Two call sites pass a value that can be empty:
  - `db.rs:1589` in `Store::open`: `std::env::var("JWT_SECRET").unwrap_or_default()`
  - `db.rs:1863` in `try_clone`: the same expression inline

  **The ordering that makes it inert.** `main.rs:92` calls `Store::open` and `main.rs:93` calls
  `try_clone`. `main.rs:136` is where `JWT_SECRET` is actually required, by
  `.expect("JWT_SECRET must be set")`, forty lines later. So with `JWT_SECRET` unset, both calls pass
  `""`. `secretbox.rs:59` has already returned `Ok(None)` for an empty or whitespace
  `TOTP_ENCRYPTION_KEY`, so `raw` is non-empty by the time line 62 runs, and `raw == ""` is false for
  every possible key. The comparison can never be true, and the guard passes silently.

  **What would have to hold for it to bite.** `JWT_SECRET` set, and `TOTP_ENCRYPTION_KEY` set to the
  same value. Then `open` reads the real secret, the comparison is true, and the process refuses to
  start as intended. So the guard works in exactly the configuration it was written for, and is inert
  only when the other variable is missing.

  **That "no live exposure" reading was wrong, corrected 2026-11-24.** It holds only for `JWT_SECRET`
  being *unset*. The reachable case is the variable set to nothing: `std::env::var` returns `Ok("")`
  for a set-but-empty variable, `Ok("")` satisfies `.expect`, so `main.rs:136` accepted it and the
  process started. It then signed every session with an empty secret while the comparison at
  `secretbox.rs:62` was false for any real key, so the guard was inert in a process that was serving.
  A line of `JWT_SECRET=` in the environment file is all it takes, and production reads one.

  Platform note, because it decides whether this was ever live here: on Linux, which production runs,
  a set-but-empty variable stays set and empty. On Windows an empty assignment removes the variable
  instead, so the same misconfiguration would have been the unset case and would have stopped the
  process. The exposure was real on the host that matters and not reproducible on the development
  machine, which is the worst combination for noticing it.

  The old read also broke two other rules at once: `.expect` is a panic outside tests, and a
  configuration fault should be an error rather than a crash.

  The remaining defect, unchanged by the correction, is that a security guard's effectiveness depended
  on a variable checked later, in a different file, by a
  different mechanism, and nothing records that dependency. It is the AUD-045 shape: the pattern, not
  the damage.

  Three edits would turn it into a live hole, none of them obviously dangerous on its own: moving
  `main`'s validation after the database opens, softening that `expect` into a default, or calling
  `Store::open` from anything that is not `main`, such as a migration tool or a fixture harness. The
  last is the likeliest, because a test harness that opens a store is an ordinary thing to write.

  Closed by reading `JWT_SECRET` once, with `?`, before the store opens, and passing it in; or by
  having `from_env` refuse an empty `jwt_secret` outright, which makes the dependency explicit at the
  point that relies on it. The second is smaller and fails closed. The guard is a fixture with
  `JWT_SECRET` unset and `TOTP_ENCRYPTION_KEY` set, asserting the open is refused rather than
  accepted.

  Not fixed. Recorded 2026-10-04, found while hunting the AUD-049 shape through `Store::open`.

- **AUD-038** The unlabelled delete removed 2,000 fewer `solar_wind` rows and 1,187 fewer `imf` rows
  than were counted 42 minutes earlier, and the difference is unexplained. Measured at 22:47 UTC on
  2026-09-22: 193,712 and 94,235 rows with a NULL source. Deleted at 23:29:40 by the migration's own
  log line: 191,712 and 93,048. Step 1 had labelled 83 rows in between, which should have been
  excluded from the delete rather than subtracted from it, so it accounts for none of the gap. The
  90-day retention boundary sweeping through 2026-06-24 was the obvious candidate, but the log line
  pulled for that window showed only the interval configuration and no purge event, so it is a
  hypothesis and not a cause. Correctness is unaffected: the delete was scoped on `source IS NULL`
  and zero unlabelled rows remain. Recorded because the arithmetic does not close, not because
  anything is known to be wrong.

- **AUD-039** Five readers had their reach silently narrowed and a test caught it, not review.
  `one_row_per_minute` takes a lower bound as its only parameter, because `WHERE observed_at > ?` is
  part of the shared SQL. `get_solar_wind_recent`, `get_imf_recent`, `get_solar_wind_latest_public`
  and the two `latest_*_raw` readers previously had no cutoff at all, and were first wired with
  `now() - 2 * 86_400`. In production that is invisible: 1440 one-minute rows is a single day.
  `a_stale_series_reads_as_empty` caught it anyway, because its fixture inserts a forty-day-old row
  and asserts it comes back. They now bind `0`, keeping exactly the reach they had. The narrowing
  may be defensible on its own, a forty-day-old point on a chart labelled recent is questionable,
  but it arrived as a side effect of plumbing rather than as a decision, which is the wrong way for
  a behaviour change to arrive. Open question: should those readers have a bound at all, and if so
  what, decided rather than inherited.

- **AUD-040** `every_component_a_cycle_writes_is_declared` checks a reconstruction, not the cycle.
  `poller.rs` builds its `series` fixture from `SERIES_FRESHNESS` directly, while the real cycle
  passes in whatever `series_health()` composed. The two drifted the moment `PRIMARY_SOURCES` joined
  the composition, and the guard failed with `noaa_solar_wind_primary is declared and never written
  by a health cycle` for a component the real cycle does write. Updating the reconstruction restored
  the guard but left the weakness: it must be edited whenever the composition changes, which is the
  thing it exists to catch. The stronger form builds the list from a real store and calls
  `series_health()`, so there is no second copy to drift. The poller test module has no store
  helper, which is the only reason it was not done that way.

- **AUD-041** The CSV export does not say which spacecraft measured a row. `get_report_csv` now
  returns one row per minute with the active row preferred, which is deterministic where it used to
  be arbitrary, but its column list is unchanged and carries no `source`. Every other Displayed
  reader carries `source` and `active` per row so a secondary reading cannot pass as the
  measurement; the CSV cannot. Adding a column changes the export format for every existing
  consumer, which is a decision about the published contract rather than about provenance, and is
  why it was left out rather than folded in.

- **AUD-036** Three tests shared a fixture whose spacing depended on how much of the current UTC day
  had elapsed, so they failed in the first minutes of a day and passed for the rest of it.

  `samples_due` in `routes.rs` divides the elapsed part of a day by the poll interval, so a day
  holding less than one 300 second interval is due nothing and is skipped. That is deliberate and the
  handler's own comment says so. The fixtures placed their samples in today and then asserted a day
  count that only holds once today has a due sample, which is about five minutes after midnight UTC.

  **All three call sites, enumerated from the source rather than from the ones observed failing:**
  - `routes.rs` `a_component_with_no_history_reports_null_not_zero`, failed 2026-10-04 at 00:03 and
    passed again by 00:07
  - `routes.rs` `history_before_the_first_sample_is_not_held_against_a_component`, failed 2026-09-23
    at 00:02, the only one this entry originally named
  - `routes.rs` `a_fixture_run_stays_inside_one_utc_day`, the helper's own property test, which never
    failed because it already anchors to a fabricated midnight

  **The entry itself was the enumeration mistake.** It described one symptom when three tests shared
  the helper, because it was written from the test that happened to fail that night rather than from a
  scan of the call sites. A finding recorded from observed output covers only what has spoken, which
  is the same error the [[feedback-enumerate-from-the-protected]] rule exists to prevent, appearing
  inside the record instead of in the code.

  **A fixture-only fix was tried first and made it worse**, which is the evidence that the cause was
  not the fixture. Moving the samples to a fabricated complete day leaves `now` in today, so today is
  counted with no operational samples: `recorded_days` went to 3 against an expected 2 and
  `uptime_pct` to 5.26 against an expected 100.

  **AUD-036 closed 2026-10-04**. The computation moved out of the handler into `uptime_report(now, interval,
  rows, first_seen)`, with the handler passing `chrono::Utc::now().timestamp()`. The arithmetic is
  unchanged: 90 lines before and after, identical once indentation is ignored, and all 236 tests
  passed without modification. `the_uptime_day_boundary_holds_at_both_ends_of_a_fabricated_day` now
  pins both ends at any hour, `recorded_days` 0 with a null percentage at midnight plus 180 seconds
  and 1 with a real figure at midnight plus 86,340. Mutation: restoring the internal clock makes it
  report 31 recorded days instead of 0 and the test fails.


- **AUD-037** `GROUP BY observed_at / {bucket}` does not bucket. DuckDB's `/` is float division, so
  `1790000000 / 900` is `1988888.88...`, distinct for every second, and the grouping groups by row
  rather than by the interval the constant names. `get_solar_wind_range` at `db.rs:2519` and
  `get_kp_range` at `db.rs:2485` both do it, so `/api/reports/solar-wind` and the Kp chart have been
  returning one point per minute whatever bucket they selected, 900, 3600 or 21600 seconds. Found on
  2026-09-23 by a fixture that put two minutes in one bucket and got two rows back.

  **This is the third instance of the same operator, and the second one was already fixed.**
  `uptime_by_day` at `db.rs:4714` uses `CAST(ts // 86400 AS BIGINT)` and its comment records the
  mechanism: with `/`, the cast rounded rather than truncated, so every health sample after midday
  was filed under the following day. So the correct operator was known, written down and applied at
  one site, and the two siblings were never looked for. That makes this a habit rather than two
  bugs, and the fix is not only the operator: it is a scan of every division inside SQL, which found
  24 candidate sites of which exactly these two are arithmetic and the rest are URL paths and units.

  Not fixed here. Changing the bucketing changes the resolution of two live chart endpoints, which
  is a separate decision from provenance.

- **AUD-014** The forecast band is still uncalibrated epistemic spread with no observation noise
  term. Six files labelled it a 95 percent confidence interval; none does now. Closing it means an
  observation noise term and recalibration, then the label. `ml/test_serve.py` pins the
  construction and deliberately does not assert coverage, since no unit test can turn a coverage
  figure into 95.

  **Measured on the checkpoint now running, 2026-09-22.** Model sha
  `061a5d30fac50c5f7e941730a37726c2bf02c008f72f484e8c01f143274760d1`, the only one that has ever
  written a band with a recorded sha, 3835 rows issued between `issued_at` 1788236182 and
  1789959066. The query selects `kp_forecast` rows with a non-null `model_sha` and pairs each with
  the nearest `kp_3h` observation within 5400 s, half that series' own cadence.

  | horizon | paired | inside | coverage | mean width Kp | mean abs err Kp | control at +12 h |
  |---------|--------|--------|----------|---------------|-----------------|------------------|
  | 3 h     | 982    | 92     | 9.4 %    | 0.189         | 0.756           | 6.6 %            |
  | 6 h     | 925    | 132    | 14.3 %   | 0.302         | 0.772           | 12.2 %           |
  | 12 h    | 912    | 148    | 16.2 %   | 0.326         | 0.809           | 13.1 %           |
  | 24 h    | 888    | 116    | 13.1 %   | 0.236         | 0.793           | 9.4 %            |

  Nominal is 95. The band is roughly a quarter of the width of the typical error at every horizon.
  The control column repeats the query with each forecast deliberately paired against an
  observation 12 hours away; it lands 2 to 4 points below the real figure, so the pairing is doing
  some work, but a band this narrow misses whether it is paired correctly or not. These figures are
  an internal record and are not published on any surface.

  **History, on the retired checkpoint.** Coverage was computed for the first time on 2026-08-31:
  **13.1 percent** over 1229 forecasts, mean width 0.405 Kp against a mean absolute error of 0.727
  Kp. AUD-032 later established that every head was trained one period beyond the lead it was
  published as, so those forecasts were also paired at a lead the model was not trained for. The
  figure describes neither the band nor the pairing in use now, and the rows behind it carry no
  `model_sha`.

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
- **AUD-020** The state token is bound to the browser now, and PKCE is not added. The cookie
  closes the login CSRF path for every provider; PKCE would add defence against code interception,
  which a confidential client with a fixed `redirect_uri` already makes hard. Left out deliberately
  rather than overlooked, and it is a small follow-on carrying the verifier in the same cookie.
- **AUD-022** The storage half is done. What remains is the Forecast page: `/api/forecast/metrics`
  now returns one entry per horizon and the page still renders a single set of figures, taken from
  the 3 h entry. Four columns, four series on the chart, and the keys in both locales are their own
  piece of work. Until then the page shows less than it stores, which is the safe direction.
- **AUD-026** Beyond Stage 3 above: there is no `cap_drop: [ALL]` and no `read_only` on any
  service, and `depends_on` is still the short form, so `condition: service_healthy` is absent and
  the backend can still start before ml has loaded its checkpoint despite ml having a healthcheck
  to wait on.
- **AUD-027** `Referrer-Policy` was named in the fix and never added, and was still absent from
  the live response on 2026-09-04. **The header half is done**: it now ships as
  `strict-origin-when-cross-origin` in `security-headers.conf`, so the token in a verification or
  reset link cannot cross an origin boundary. What remains open under this ID is the report-only
  CSP above, which is a separate deferral.
- **AUD-057** `get_token_version` turns a failing query into version 0, and the direction is fail
  closed. `db.rs:3053`, with the `.unwrap_or(0)` at `db.rs:3061`.

  The statement already reads `COALESCE(token_version, 0)`, so the `unwrap_or` is not catching a
  NULL. What it catches is a query that failed or an account row that is not there. `auth.rs:1361`
  compares `claims.ver != current`, so a zero rejects every session whose token carries a non-zero
  version: a mass logout rather than a stale session accepted, and no line saying why.

  Found 2026-11-24 in the crate-wide hunt recorded under AUD-052, where the table placed it at
  `db.rs:3040`; the AUD-052 and AUD-053 commits moved the line. Identified here so each of the six
  sites is decided on its own rather than as a row in another finding's table.

- **AUD-058** `current_token_version` swallows the same error through `.unwrap_or_default()`.
  `auth.rs:246`.

  The same call, the same zero, the same fail-closed direction as AUD-057, spelled with
  `unwrap_or_default` instead of `unwrap_or(0)`. Separate identifier because it is a separate call
  site with its own caller, and fixing one says nothing about the other.

- **AUD-059** `resolve_token_version` swallows the error and then caches the zero, so the effect
  outlives the fault. `rate_limit.rs:279`.

  The value is inserted into `UsageEntry.token_version` in the usage counter, which the function's
  own doc comment describes as the hazard: every request takes the cached entry. So one failed query
  logs the account out until the entry is evicted or `clear_user_cache` is called, where AUD-057 and
  AUD-058 fail once per request. Same direction, different duration, which is why it is its own
  finding.

- **AUD-060** A database error stops threshold emails silently. `poller.rs:803`.

  `list_enabled_email_alerts().unwrap_or_default()` turns a failing query into an empty subscriber
  list, so `dispatch_email_alerts` iterates nothing, returns zero, and the cycle looks like a quiet
  hour. Nothing distinguishes that from nobody having subscribed, and no line is written. This is
  the only one of the six that is silent in both directions, which is why the order takes it first.

- **AUD-061** The same cycle turns a failing reading query into "no reading". `poller.rs:801` and
  `poller.rs:802`.

  `latest_kp_raw().unwrap_or(None)` and `latest_solar_wind_speed_raw().unwrap_or(None)` give
  `alert_lines` a `None`, which produces no lines, which means no alert for any subscriber whose
  threshold the real reading would have crossed. Found only because they sit two lines above
  AUD-060: a sweep written for `.unwrap_or(0)` and `.unwrap_or_default()` does not match
  `.unwrap_or(None)`.

- **AUD-062** A database error downgrades the caller to the free plan, and caches that too.
  `rate_limit.rs:281`.

  `get_user_plan(email).unwrap_or_else(|_| "free".to_string())` writes "free" into the same
  `UsageEntry` as AUD-059, so the free quota and the free billing period apply to a paying account
  until the entry is evicted. Fail closed for entitlements and wrong for the customer. The third
  spelling the AUD-052 sweep did not look for.

- **AUD-063** `ML_TIMEOUT` is read with two different defaults, so one variable means two things
  depending on which path reads it. 10 seconds at `poller.rs:668`, 5 at `routes.rs:874`.

  Both parse the same variable and both fall back to a literal. Unset, the poller waits twice as
  long as the on-demand forecast handler for the same ML service. Found 2026-11-24 in the AUD-052
  sweep, which recorded the second site at `routes.rs:774` before the AUD-052 commit moved it.
  Neither default is documented as the default, and the project's own instruction file lists
  `ML_TIMEOUT` once.

- **AUD-064** The monitor saw the 2026-10-27 outage and the owner did not. Not a change to this
  repository.

  Carried out of AUD-056, where it is measured: UptimeRobot polled throughout and its 200s resume in
  the access log at `01:00:49`, so the monitor had the outage from `08:12`, and the owner learned of
  it 16 h 46 m later from a browser. What is not known is which link failed, whether the alert
  contact, the notification channel or the monitor's own alerting configuration. Identified so that
  the question has somewhere to be answered rather than living inside another finding's closing
  paragraph.

- **AUD-065** The `ml test` gate step cannot run on this machine, so it has never asserted anything
  here. Measured 2026-11-24.

  `python -m unittest discover -s ml -p "test_*.py"` runs 5 tests and errors on one:
  `test_serve` fails at `import serve`, which fails at `import torch.nn` with
  `ModuleNotFoundError: No module named 'torch.nn'`. The cause is more specific than a missing
  install. `site-packages/torch` exists as a directory with no `__init__.py`, so `import torch`
  succeeds as a namespace package with `__file__` of `None` while every submodule fails. numpy 2.2.6
  is present; fastapi, uvicorn, pandas, sklearn and pydantic are not. The local interpreter is
  Python 3.10.11 and `ml/requirements.txt` targets 3.11.

  So the step is red for an environment reason, not a code one, and `test_published_claims` is the
  only ML coverage the gate has actually been running.

  **AUD-065 closed 2026-11-25**, commit `843d8b0`, by the move to WSL. The gate's Python steps now run
  on a venv at `.venv`, built with Python 3.11.18 from `ml/requirements.txt`, which matches the
  `python:3.11-slim` base of `ml/Dockerfile`. Its `torch` is `2.10.0+cpu` with a real `__init__.py`,
  and fastapi, uvicorn, pandas and pyarrow are installed. The gate run of 2026-11-25T18:12:37Z at
  `20f74ae` reported `PASS ml test`, with the venv change in the tree and not yet committed.
  `843d8b0` commits it. Run directly, `.venv/bin/python -m unittest discover -s ml -p "test_*.py"`
  runs 11 tests and reports `OK`, against 5 tests and one import error before. A missing venv now
  fails all three Python steps with a message that names the path and the build command. That was
  shown in a scratch clone with no venv, where removing the check turned the message back into a bare
  `No such file or directory`.

  The closure's other half, the `CLOSED` declaration in `backend/tests/backlog_structure.rs`, was
  mutation tested on 2026-11-26 in a temporary worktree at `a97a35a`, one mutant at a time. The
  unmutated tree passes. Each mutant failed `every_closure_is_declared_and_written_in_the_one_form` at
  the assertion written for it. A closure marker for `AUD-071`, which `CLOSED` does not declare, failed
  on `marked closed in docs/BACKLOG.md but not declared here`. A second `AUD-070` marker failed on `a
  closure marker appears more than once`. A `CLOSED` entry for `AUD-071` with no marker failed on
  `declared closed but not marked that way`, and so did the `AUD-070` entry with its date moved to
  2026-11-24. Removing `AUD-070` from `FINDINGS` failed on `closed findings missing from FINDINGS`, and
  also failed `every_declared_finding_still_has_a_bullet`, which is right because it is an undeclared
  finding as well. A parser that matches no marker failed on the floor, `found 0 closure markers`.
  Those assertions run in sequence, so each mutant reaching its own assertion shows that the earlier
  ones passed it and none masked it.

- **AUD-066** The `frontend test` gate step stopped running when Node moved to 24. Measured
  2026-11-24.

  `gate.sh:71` runs `node --test src/lib/`. Under Node v24.22.3 that path is resolved as a module
  rather than expanded as a test directory, and the step exits 1 with
  `Error: Cannot find module 'C:\astraeus\frontend\src\lib'`. No test in `src/lib` has run since
  the upgrade, and the failure looks like a failing test rather than a runner that never started.

  **AUD-066 closed 2026-11-26**, commit `0d4b18e`. Node reads each argument to `node --test` as a
  glob pattern, so `src/lib/` matched the directory itself and Node loaded it as a module. Reproduced
  on WSL under Node v24.22.1 with the same `MODULE_NOT_FOUND`. A quoted glob ran the 8 tests, and a
  glob that matched nothing reported `tests 0` and exited 0, so a glob alone would have turned a
  runner that finds nothing into a pass.

  The step is now `scripts/lib/frontend-test.sh`. It enumerates the test files under `src` from the
  tree, fails when there are none, passes them to Node by name, and fails unless the files that the
  junit reporter names equal that list. A file that loads and declares no test is reported by Node as
  one synthetic case named after its own path, so such cases do not count as a file having run.

  Mutants, each in a throwaway clone, each against the script and then against the script with its
  guard removed. No test files fails on the empty list check and passes without it. A test file that
  declares no test fails on the file comparison and passes without it. The old directory argument
  fails with the comparison naming `src/lib/utils.test.js` as unreported. A second test file in the
  tree that Node is not given fails, naming that file. The unmutated script reports 1 test file in
  the tree and 1 reported, and the gate at `9cd7d27` with the change in the tree went from 2 of 17
  failing to 1 of 17, with `advisories` the one left.

- **AUD-067** The gate measures the working tree, so a commit can pass it and not compile. Found
  2026-11-24, and it had already happened.

  `a32835f` was pushed and deployed on 2026-11-24 with twelve one-argument `Store::open(":memory:")`
  calls still in it: four in `auth.rs`, two in `oauth.rs`, two in `poller.rs` and four in
  `routes.rs`, all inside test modules. AUD-053 gave `Store::open` a second parameter, so `cargo test`
  cannot compile that commit. The gate at that sha reported `rust test` passing in 97 seconds, and it
  was telling the truth about the tree in front of it, which held the two-argument form uncommitted.

  **The shipped binary is unaffected and that is not luck.** `cargo build --release` does not compile
  `#[cfg(test)]` modules and the Dockerfile builds release, so the image carried the correct code and
  the deploy's health checks measured something real. Only the test targets are broken, which is the
  half nobody runs on the server.

  **How the omission stayed silent.** The standing rule against `git add -A` means files are staged by
  name. `6a96f6f` named `db.rs` and `main.rs`, the two files the fix was about, and the four files that
  changed only because a signature moved were never named. The rule that stops an accidental commit
  also makes an incomplete one quiet, and nothing between the edit and the push compares the staged
  set against what the change needs.

  **This is the AUD-054 shape for the sixth time.** Two artifacts, the working tree and the commit,
  and the check reads the one that is not pushed.

  Two repairs, neither written. A gate step that fails when `git status --porcelain` is not empty
  before the Rust steps run: cheap, catches exactly this, and noisy during ordinary work because the
  gate is run mid-change on purpose. Or run the Rust steps against a clean export of `HEAD`: correct
  whatever the tree holds, and slower, because that export cannot share the warm target directory.

  The twelve call sites are committed in `1ea602e`. The gate change is not written.

- **AUD-068** `duckdb = "1"` lets a plain `cargo update` replace the database engine. Found
  2026-11-24 while reading the version scheme for the zip advisories.

  The duckdb-rs README states the scheme at its line 137: from DuckDB `v1.5.0` the crate version
  encodes the engine in its second semver component, format `1.MAJOR_MINOR_PATCH.x`. So the locked
  `1.10502.0` is DuckDB **1.5.2**, and the newest published `1.20001.0` is DuckDB **2.0.1**. Both
  satisfy `^1`, which is what `duckdb = { version = "1", features = ["bundled"] }` in
  `backend/Cargo.toml` asks for.

  So one `cargo update` with no arguments moves the storage engine across a major version, and the
  only visible trace is a line in `Cargo.lock`. DuckDB writes its own file format version into the
  database, and a newer engine can read an older file while the reverse does not hold, so the step is
  one way on a file that holds production data.

  The requirement is the defect rather than the version. A caret range on a crate whose version
  encodes a vendored C++ engine says "any engine" while reading like a normal Rust dependency bound.
  Fixed in the same round by pinning `~1.10506`, which allows the patch component to move and refuses
  the engine.

- **AUD-069** Two advisories are allowlisted in `backend/.cargo/audit.toml`, which the standing rules
  no longer permit. Found 2026-11-24 while reading that file for the jsonwebtoken bump.

  The rule now reads that no advisory is ever accepted, allowlisted or ignored, and that each is fixed
  at its source by a dependency update, a crate replacement or a fork. The file carries two `ignore`
  entries written under the earlier rule, both with careful reachability arguments:

  - `RUSTSEC-2023-0071`, rsa, reached through jsonwebtoken. The argument is behavioural rather than
    graph based: rsa is compiled, and every mint site passes `Header::default()` while every verify
    site builds on `Validation::default()`, both HS256 only, so an RSA algorithm is rejected before
    any rsa code runs. The jsonwebtoken 12 bump in this round is the occasion to re-measure it.
  - `RUSTSEC-2026-0235`, rkyv, reached through rust_decimal under duckdb. The argument is that
    `cargo tree -i rkyv --target all` prints nothing, so it is not compiled, and the file says plainly
    that this is the weaker of the two because rust_decimal is compiled and one feature change makes
    rkyv live.

  No decision taken and nothing changed. The entries are recorded here because a rule violation
  sitting in a tracked file with a persuasive comment next to it is the kind of thing that survives
  every future audit by looking already considered.

  **AUD-069 closed 2026-11-25**. Both entries are gone and the list is empty. Neither was argued
  away: `RUSTSEC-2023-0071` went on 2026-11-24 when jsonwebtoken 12 brought `rsa 0.10.0`, the patched
  version, and `RUSTSEC-2026-0235` goes now because rkyv is no longer a dependency at all.

  **rkyv left the lockfile on 2026-11-25, measured rather than assumed.** `rkyv` is absent from
  `backend/Cargo.lock`, and so are `bytecheck`, `rend`, `munge`, `ptr_meta` and `rancor`.
  `cargo tree -i rkyv --target all -e normal` answers
  `error: package ID specification 'rkyv' did not match any packages`, which is a different answer
  from the `nothing to print` the 2026-09-04 measurement got: then it was in the graph and
  unreachable, now it is not in the graph.

  The positive control matters here, because a tool that cannot find anything looks the same as a
  clean graph. The same command for `chrono` prints the arrow family and this crate, and for `rsa`
  prints `rsa v0.10.0 -> jsonwebtoken v12.0.0 -> backend`, so the command works and the absence is
  real.

  **What removed it was `c122a18`, the duckdb engine pin, and the mechanism is in the two manifests.**
  `duckdb-1.10502.0/Cargo.toml` line 221 declares `[dependencies.rust_decimal] version = "1.14"` with
  no `optional = true`, so rust_decimal 1.41.0 was an unconditional dependency and its own optional
  `rkyv` feature put rkyv 0.7.46 in the lockfile, a lockfile holding every optional dependency whether
  the feature is on or not. `duckdb-1.10506.1/Cargo.toml` line 236 declares the same dependency with
  `optional = true`, gated behind a `rust_decimal` feature that is also part of `extensions-full`.
  This crate asks for `["bundled"]`, so the feature is off, rust_decimal left the lockfile, and rkyv
  left with it.

  So the question the decision asked, whether a semver compatible version of the parent drops rkyv 0.7
  or moves to 0.8.17, has an answer that does not need either: the parent itself is gone.

  **With the list empty, `cargo audit` reports 0 vulnerabilities and one warning, chrono.** Identical
  to the run with the entry in place, which is what a dead entry means.

- **AUD-070** Two uptime tests still take their day from the real clock, so the gate is unreliable
  around midnight UTC. Found 2026-11-25 when it failed the gate.

  The `rust test` step of the tokio gate run failed with two of 254 tests:
  `routes::mcp_tests::a_component_with_no_history_reports_null_not_zero` at `routes.rs:2413`, which
  wanted `recorded_days` 2 and got 1, and
  `routes::mcp_tests::history_before_the_first_sample_is_not_held_against_a_component` at
  `routes.rs:2609`, which wanted `uptime_pct` 100.0 and got `Null`. That step ran from about 23:45 to
  00:03 UTC. The same tree, with the same lock, passed all 31 `mcp_tests` at 00:05 and the whole gate
  at 00:07.

  **The mechanism, read from the code rather than inferred from the timing.** Both fixtures call
  `samples_within_today(now_ts, count)` with a `now_ts` they read themselves, then assert on the
  `uptime` handler, which reads its own `chrono::Utc::now()`. Two clocks, so the day each one derives
  differs whenever midnight falls between the fixture and the call. `samples_due` also skips a day
  holding less than one poll interval, so the first 300 seconds of a UTC day cannot satisfy "samples
  inside today" at all.

  **This is AUD-036 surviving its own fix.** That fix introduced `uptime_report(now, interval, rows,
  first)` with `now` as a parameter and converted three tests to a fabricated day, which is what
  `the_uptime_day_boundary_holds_at_both_ends_of_a_fabricated_day` does and why its comment names
  AUD-036. These two were left on the real clock through the handler, so the seam exists and they do
  not use it.

  **Not fixed in this round, and the reason is a choice rather than effort.** Converting them to
  `uptime_report` takes the database aggregation out of their coverage, which is part of what they
  assert. Keeping the database path means giving the handler an injectable clock, a production change
  made for a test. Which of those to pay is the owner's call, and the round in progress is the
  advisory round.

  What makes it worth an identifier rather than a retry: the unreliable window is midnight UTC, which
  is exactly when a long gate run is most likely to have been left running unattended, and a gate that
  fails for the hour rather than for the change teaches everyone to re-run it instead of reading it.

  **AUD-070 closed 2026-11-25**, commit `94b2ad4`, by the injectable clock. The owner took that side
  of the choice on 2026-11-25: keep the database coverage and pay the production change.

  **The clock reads were enumerated from the code, and there were two, not one.** `AppState` carries
  `pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>`, `AppState::new` sets the real clock, and the
  handler reads it once. The second read was not in the handler at all: `db::uptime_by_day` computed
  its own ninety day window from `now()` in `db.rs`, so the window and the day arithmetic could
  straddle a midnight even with the handler's read injected. It now takes `now` as a parameter from
  the same read. `cached` reads `Instant::now()` for its five minute TTL, which is monotonic and
  carries no UTC day, so it is not part of this and is left alone. Nothing else in the path reads a
  clock: `health_interval_secs` reads an environment variable and `health_first_sample` reads a
  `MIN(ts)`.

  **Five tests had the defect, not two.** The gate failed on two of them, which is the hour deciding
  which instance speaks rather than which instances exist. The shape is the literal pair
  `let state = test_state();` followed by `let now_ts = chrono::Utc::now().timestamp();`, and it
  appeared exactly five times: `a_component_with_no_history_reports_null_not_zero`,
  `silence_counts_against_a_component_that_had_started_recording`,
  `history_before_the_first_sample_is_not_held_against_a_component`,
  `a_day_of_samples_fills_exactly_one_calendar_cell` and
  `a_liveness_component_is_measured_by_presence`. All five now take `state_at(now)`.

  **The fabricated day is 2024-10-04, and the date is doing work.** `FIXED_MIDNIGHT` is
  `1_728_000_000`, which is `20000 * 86400` and so a UTC midnight, chosen far enough in the past that
  `uptime_by_day`'s ninety day window ending at the real clock cannot contain it. A clock read that is
  not the injected one therefore returns no rows at all and the assertions fail. Picking a day near
  today would have left a mutant that survives for as long as the fabricated day stays inside the real
  window, which is the kind of test that passes today and proves nothing in December.

  **Two new tests.** `the_same_samples_report_the_same_uptime_from_both_sides_of_midnight` puts 288
  operational samples in one fabricated day and asks the handler twice, at 23:59:40 and at 00:00:20,
  through the real route, the real query and the database. Both sides must report 100 and one recorded
  day, and the cell must move from index 89 to index 88 while the new day stays `no_data` because
  twenty seconds is under one 300 second interval. The percentages are asserted against 100 and not
  only against each other, because two nulls are equal and that comparison alone would pass with the
  query returning nothing.

  `the_default_clock_reads_the_real_time` asserts that `AppState::new` hands out a clock within two
  seconds of the real one. Every other uptime test replaces the clock, so without this a wrong
  production default would file the live strip under the wrong day and no test would notice. The rule
  and its application, tested apart.

  **Mutation round, five mutants, in a worktree created for it and removed after.** Each was run
  against its declared guard alone, so nothing else could have done the killing.

  | mutant | the defect planted | declared guard | result |
  |---|---|---|---|
  | M1 | the handler reads `Utc::now()` again instead of the injected clock | `..._from_both_sides_of_midnight` | CAUGHT |
  | M2 | `uptime_by_day` is handed `Utc::now()` while the arithmetic gets the injected value | the same | CAUGHT |
  | M3 | `let since = now - days * 86_400` becomes `let since = now` | the same | CAUGHT |
  | M4 | `uptime_report` is handed `now + 86_400`, a day later than the window | the same | CAUGHT |
  | M5 | `AppState::new` wires a clock a day ahead | `the_default_clock_reads_the_real_time` | CAUGHT |

  All five compiled, so each measured something, and the baseline run showed both guards green before
  any mutant was planted. The control at the end asked for a test name that does not exist and got
  `0 passed; 0 failed`, which is the shape a silently empty filter leaves and is why the script treats
  no result line as a failure rather than a pass.

  One limit worth stating rather than leaving to be assumed: the second run for each mutant, the one
  that reports what else noticed, filtered on `uptime` and so covered four tests, not the five
  converted fixtures whose names do not contain that word. So `nothing else failed` is a statement
  about those four.


- **AUD-071** The uptime response is cached under a key that carries no UTC day, so the status strip
  can show the wrong day for up to five minutes after midnight. Found 2026-11-25 while giving the
  handler an injected clock: the two sided test asks the handler twice and only avoids the cache
  because each side is a separate state with a cache of its own.

  `routes.rs` wraps the handler in `cached(&s.cache, "uptime_90d", Duration::from_secs(300), ...)`.
  The key is the literal `uptime_90d` and the entry lives for 300 seconds. A response computed at
  23:59:50 is therefore served until 00:04:50, and in that window the ninety cells are yesterday's:
  the last cell is the previous day and the new day has no cell at all. The percentages are right for
  the day they were computed on, so nothing reads as an outage, and the strip is simply one place out
  of step with the clock on the viewer's wall.

  Small, and recorded rather than fixed because the fix is a choice: put the UTC day in the cache key,
  which evicts once a day at the boundary, or shorten the TTL near midnight, which is worse. Neither
  belongs in a round whose subject was the test clock, and a five minute lag on a ninety day strip is
  not an incident.

  It is the fourth instance of the day boundary shape, after AUD-036, AUD-070 and the `uptime_by_day`
  window this round found inside the database layer.

- **AUD-072** The git hooks AUD-055 describes no longer exist, so nothing checks a commit or a push
  when it is made. Found 2026-11-25 while moving the project from the Windows checkout to WSL.

  Measured 2026-11-25 on both machines. `core.hooksPath` is not set in the global, system or
  repository scope. In WSL `git config --show-origin --get-all core.hooksPath` prints nothing. On
  Windows the user `.gitconfig`, the Git for Windows system `gitconfig` and the `.git/config` of the
  Windows checkout were read directly, and none of them contains `hooksPath`. Neither checkout has
  anything in `.git/hooks` except the `.sample` files git creates. A search by file name for
  `naming_delegate` over both home directories finds nothing. The Windows search skipped `AppData`,
  `node_modules`, `target`, `.rustup` and `.cargo`. Each search had a positive control. The same
  expression found a file planted in a scratch directory, and on Windows it found the deploy key and
  the hook sample files across 96,594 files.

  The function is still in tracked code. `scripts/lib/naming.sh` line 82 defines `naming_delegate`.
  The hook files that called it and the configuration that pointed git at them are what is gone.

  **What still holds.** The `naming rule` gate step and its control run on every gate. That step is
  `naming.sh --gate`, which scans tracked paths and tracked content only. It does not read commit
  messages, author or committer identity, or tag annotations, which the `pre-push` hook covered. So a
  banned name in a tracked file is still stopped by a gate run before a push. A banned name in a
  commit message, a trailer, a wrong identity, a dash in a subject or a missing prefix is stopped by
  nothing, at commit time or at push time.

  **When.** The date is bounded, not known. AUD-055 read the three hook files on 2026-10-05. A local decision
  record in another project, which is not in this repository, records an edit to the shared
  `pre-push` on 2026-10-15, and records on 2026-10-23 that the shared hooks directory under the
  Windows user profile was missing and that no scope set `core.hooksPath`. By that record the hooks
  went between 2026-10-15 and 2026-10-23. It is a secondary source for this repository.

  **What the gap already let through, audited backward on 2026-11-25.** 31 commits in this repository
  are dated after 2026-10-15. The first of them is `310ac28` on 2026-11-24, so none falls inside the
  window and every one of the 31 was made with no hook. All 31 have a one line message with a
  conventional prefix, no trailer, no en dash, em dash or minus sign, no banned name in the message
  or the identity, and the identity `ChronoCoders <altug@bytus.io>` as both author and committer. A
  keyword scan for cleanup subjects (format, whitespace, punctuation, dash, typo, tidy) found none.
  No tag was created after 2026-05-31. Each filter was shown to fire on a planted message before its
  empty result was trusted. So the gap has cost nothing yet, and only habit has held it.

  The hooks are not rebuilt here, because they are shared across projects and rebuilding them is a
  separate task.

- **AUD-073** A Cloudflare Global API Key and a DNS token sat in plain text in a local agent memory
  note outside this repository, and the Global API Key had been used from it to purge the cache.
  Found 2026-11-26 while looking for how the post deploy purge of 2026-11-25 was done.

  The session log of the Windows checkout shows that on 2026-11-25 at 09:38Z a session read the note
  `reference_cloudflare.md` in the agent memory for that checkout, wrote a `purge.sh` into its
  scratchpad, and called `purge_cache` with `X-Auth-Email` and `X-Auth-Key`, which is Global API Key
  authentication. The note also held a DNS only token. Searching that log printed both values in full
  into the 2026-11-26 session, so they also reached that session's transcript.

  **AUD-073 closed 2026-11-26**, by the owner rotating and cleaning, with this evidence as the owner
  reported it. The Global API Key was changed in the dashboard, and `GET /user` with the old key now
  returns 9103, `Unknown X-Auth-Key or X-Auth-Email`. The DNS only token was already dead:
  `/user/tokens/verify` and `/accounts/<id>/tokens/verify` both return 1000, `Invalid API Token`, so
  there was nothing to rotate. The note was kept for its zone facts, with every string of 30 or more
  key characters masked, and the scratchpad `purge.sh` was deleted. The other agent memory notes were
  scanned for long key shaped strings and none holds a live secret.

  The purge no longer needs a key at all. Since 2026-11-26 it goes through the Cloudflare MCP server
  connected to the agent session, as `docs/RUNBOOK.md` describes, and that section says not to fall
  back to the Global API Key.

  **`/opt/astraeusio/.cloudflare-key` on the host, deleted 2026-11-26 by the owner's decision.** Its
  contents were never read. Before deletion it was mode 600, 142 bytes, modified
  `2026-05-31 21:45:37 UTC`, and nothing referenced it. The root crontab, the systemd unit
  directories and `/etc` do not name it, no tracked file outside `docs/` names it, and no process held
  it open. The positive control for the same search found the three tracked scripts that read the
  sibling `.r2-s3-key`. Whatever it held was either the rotated key or unknown, and neither belongs on
  the host. It was removed with `rm` on that path and the path was then confirmed absent. Two
  untracked root scripts from the 2026-10-04 history rewrite listed it among the files that cannot be
  recreated, and both lists now omit it: `/root/align_clone.sh` lines 8, 74 and 75, where five became
  four, and the `grep -E` pattern on line 40 of `/root/untracked_backup.sh`. Both still pass
  `bash -n` and keep their modes.

  **Still on the host.** Two copies with the same size, mode and mtime remain, not hashed, in
  `/root/astraeusio-untracked-backup/` and `/root/astraeusio-untracked-20261004T005000Z/files/`, both
  made by `untracked_backup.sh` before the history rewrite. They were not named for deletion and are
  left for the owner. The same reason applies to them, so until they go the credential is still on
  the host.

  **The correction.** The 2026-09-01 entry under Operational said every ops script reads from
  `.cloudflare-key`. No tracked script does, and that sentence now says so.

- **AUD-074** The APOD panel shows NASA's site logo and a video fallback sentence as the picture of
  the day, and nothing on our side can reject that record or replace it once NASA corrects it. Found
  by the owner 2026-11-26.

  NASA's APOD API returned a broken record for 2026-11-26: title `NASA Science`, and `url` and
  `hdurl` both `https://science.nasa.gov/wp-content/themes/nasa-child/assets/images/nasa-logo@2x.png`,
  the site logo. The live dashboard shows that title, and as the explanation the text `To view this
  video please enable JavaScript, and consider upgrading to a web browser that supports HTML5 video`.
  NASA's API now returns the real explanation for that date, with `url` and title still the logo and
  `NASA Science`, so the stored row was written from an earlier, worse response.

  Two defects on our side. First, `fetch_apod` accepts any record, and nothing checks that it is a
  real picture of the day. Second, `insert_apod` updates only `fetched_at` on `ON CONFLICT (date)`, so
  a record NASA corrects later never replaces the one stored. The live dashboard is the evidence.

  The round, decided by the owner the same day. Measure first: fetch the last 90 days with
  `start_date` and `end_date` and report per day the media type, the url host and path prefix, and
  whether title, url or explanation look like site chrome rather than content. Find when the broken
  records started and how many there are, and look for a public report such as an issue on NASA's
  apod-api repository. Derive the validation rule from that data, because a rule that only matches
  `NASA Science` or this one URL is not the fix. Validate at ingest: a failing record is not
  written, it is logged at WARN with the date and the reason, and the last good record stays the one
  served. The `nasa_apod` freshness check then goes stale on its own if the feed stays broken, and
  its threshold is not changed. `insert_apod` updates every content column on conflict. Every branch
  of the validator gets its own test, enumerated from the code, with the 2026-11-26 record and the
  video fallback explanation as fixtures, and a mutation round names the killing assertion for each
  mutant. Report every stored APOD row that fails the new validator by date and reason, and change
  none of them.

- **AUD-075** The people in space list counts two entries that are not people. Found by the owner
  2026-11-26, verified the same day.

  `astros.rs` asks Launch Library 2 for `astronaut/?in_space=true&limit=20` and maps every result
  through without reading its `type`, and passes LL2's own `count` on as the head count. Live on
  2026-11-26 the response held 15 entries, two of type `Non-Human`: `Starman` (SpX, status Active),
  the mannequin in the Tesla Roadster, and `Laika` (CCCP, status Deceased). So 13 people are shown as
  15. The `limit=20` page also truncates the list silently once more than 20 are in space, while the
  count stays whole.

- **AUD-076** Two i18n keys the dashboard uses exist in no locale, so the Kp 3 hour card shows the raw
  keys. Found by the owner 2026-11-26, verified the same day.

  Enumerated from the code: 570 distinct literal keys in `t('...')` calls under `frontend/src`,
  checked against both locales, with i18next plural forms (`_one`, `_other`) counted as present.
  Exactly two are missing, from `en` and `tr` alike: `metrics.kp3h` (`App.jsx:195`) and
  `metrics.kp3hPeriod` (`App.jsx:198`). `e8078d1` of 2026-09-21 introduced both calls, and no
  commit has ever added `kp3hPeriod` to a locale file, so the card has shown raw keys since then. The
  two locales carry the same 843 keys. Not checkable by this method: 48 template literal keys and two
  string concatenations (`apiKeys.state.` and `pricing.tv.`), which the check must resolve from the
  values the code can produce.

- **AUD-077** The NEO list labelled as a 7 day window serves two weeks, centred on today. Found by the
  owner 2026-11-26, verified in the code the same day.

  `get_neo_recent` keeps rows whose `fetched_at` is within 7 days, not rows whose approach date is in
  a window. Each poll fetches today to today plus 7 and refreshes `fetched_at` on every row it sees,
  so a passed approach stays until its last fetch is a week old. The served span is therefore today
  minus 7 to today plus 7, which on 2026-11-26 is 2026-11-19 to 2026-12-03, the span the owner saw.
  The count of 80 was not confirmed from data. The neo insert has the same `ON CONFLICT ... DO UPDATE
  SET fetched_at` shape as the second APOD defect, so a corrected NEO record is never stored either.

- **AUD-078** Times are shown in the viewer's zone in some places, in UTC in others, and once labelled
  UTC while in the viewer's zone. Found by the owner 2026-11-26 on the ISS card, enumerated from the
  code the same day.

  Every place `frontend/src` formats a time, and what it shows:

  | place | zone | says so |
  |---|---|---|
  | ISS `Updated` (`IssPanel.jsx:118`) | viewer's, `toLocaleTimeString` | labelled `UTC` by `iss.updated`, which is wrong |
  | Billing reset (`BillingPage.jsx:8`) | viewer's | no |
  | NOAA alert times (`AlertsList.jsx:21`) | viewer's, and a `T` form with no zone is read as local | no |
  | Status page `Updated at` (`StatusPage.jsx:281`) | viewer's | no |
  | Anomaly times (`AnomalyPanel.jsx:14`) | UTC | yes |
  | Event times (`EventsPage.jsx:11`) | UTC | yes |
  | API key times (`ApiKeysPage.jsx:14`) | UTC | yes |
  | Reports times (`ReportsPage.jsx:129`) | UTC | yes |
  | Header clock (`App.jsx:39`) | UTC | yes |
  | Kp 3 hour period end (`utils.js:111`) | UTC | yes |
  | Forecast chart axis (`ForecastPage.jsx:221`) | UTC | no |
  | Reports chart axis (`ReportsPage.jsx:37`) | wrong | no |

  The last row is a wrong value, not a missing label. The API sends `time_tag` with no zone, for
  example `2026-11-26T10:45:00`, and `new Date(time_tag)` reads that as the viewer's local time.
  `ReportsPage.jsx:221` parses it that way and the axis then prints UTC hours, so a viewer at UTC
  minus 5 sees every tick five hours late. The four live charts parse the same way but print no time
  axis, so there it only shifts a window that is relative anyway.

- **AUD-079** Ten published places say the forecast model was trained on NOAA data, and it was trained
  on GFZ data. Found by the owner 2026-11-26 on the forecast page, swept the same day. It is finding
  C-1 of the 2026-09 audit again.

  `ml/download_kp.py` downloads one file, `Kp_ap_Ap_SN_F107_since_1932.txt` from GFZ Potsdam, which
  holds Kp, sunspot number and F10.7. Nothing under `ml/` fetches from NOAA or SWPC. Still claiming
  NOAA: `forecastPage.modelDesc`, `about.missionP3` and `landing.techTrainingVal` in `en.json` and `tr.json`
  (six), the blog post in both languages at `posts.js:27` and `:73`, which also says the data was
  downloaded from SWPC archives, `agent-skills/index.json:14`, and `README.md:123`, which says
  `GFZ Potsdam / NOAA`. Correct already: the agent skill's `SKILL.md` and `webmcp-init.js`.
  `ml/test_published_claims.py` guards `SKILL.md` alone, which is how the others drifted.

- **AUD-080** The forecast page showed `Storms in window 0` while its chart showed the Kp 6 storm of
  2026-11-19. Found by the owner 2026-11-26. Not confirmed or refuted: it needs the stored forecast
  rows, which were not queried.

  What the code shows. The count and the chart ask for the same range (7 days by default) and the
  same horizon (3 h), and pair forecasts with `kp_3h` the same way. The one difference is that the
  metrics query keeps only rows with a `model_sha`, and every forecast written now carries one. NOAA's
  feed dates the storm's K5 warning and alert at 01:00 and 01:08 UTC on 2026-11-19, so a 7 day
  window viewed around 10:40 UTC on 2026-11-26 starts after them, and the storm may sit at or past
  the window edge. Separately, `n_storms` counts forecast rows paired with an observed Kp of 5 or
  more, not storms. Forecasts are written every 30 minutes, so one storm period can count several
  times, and the label `Storms in window` does not describe what is counted.

- **AUD-081** One event raises many anomaly alerts, and X-ray alerts appear three hours after their
  readings. Found by the owner 2026-11-26. The cause is read from the code, and the stored
  `detected_at` values were not queried to confirm it.

  Every check in `anomaly.rs` keys its alert on `(anomaly_type, source_ref)` with `source_ref` the
  `time_tag` of the triggering reading. While Kp stays at or above 5, each new one minute reading is a
  new key, so a storm raises one alert per reading: about fifteen in two hours on 2026-11-19.
  `check_xray` takes the peak over a sliding 3 hour window. Once the true peak leaves the window, the
  highest remaining reading becomes the new peak, a new `time_tag` and a new alert. At 16:24 the
  window starts at 13:24, so the decaying readings from 13:25 to 13:59 each became the window maximum
  in turn, one per minute. That is the eight flare alerts a minute apart, and it is also why they are
  dated three hours after their readings. One mechanism, with no detection lag.

- **AUD-082** Every NOAA space weather alert is shown as INFO under its raw product code. Found by the
  owner 2026-11-26, verified the same day.

  `AlertsList.jsx` grades severity by testing `product_id` against `/WAR/`, `/ALT/` and `/SUM/`.
  NOAA's live feed on 2026-11-26 held 61 alerts with ids such as `EF3A`, `K04W`, `A20F`, `K05A`,
  `K05W`, `K06W` and `TIIA`, and none contains those letters. They belong to the message codes inside
  the text, such as `WARK05`, `ALTK05` and `WATA20`. So every alert falls through to INFO, the K5 and
  K6 warnings included, and the card prints the raw id beside the raw message code.

- **AUD-083** Three pieces of copy describe something other than what is served. Found by the owner
  2026-11-26, verified the same day.

  The API key page lists `/api/kp-forecast` as `ML Kp forecast (+3 h, LSTM)` at `ApiKeysPage.jsx:33`,
  while four horizons are served. The docs page (`docs.epForecast`) and the 3 hour label
  `forecast.predictedKp` are correct. The exoplanet card's `Total` is `planets.length`, and the query
  is `ORDER BY disc_year DESC LIMIT 100`, so it is the 100 most recently discovered and not a total.
  Its `Sample - closest orbits` table is `planets.slice(0, 8)` of that list, the newest discoveries,
  not sorted by orbital period.

- **AUD-084** The Enterprise plan advertises dedicated infrastructure and a custom SLA, and the service
  runs on one shared host with no SLA. A claim on a sold plan that is not true. Found by the owner
  2026-11-26, swept the same day.

  `lib/plans.js:23` gives Enterprise `dedicated` (`Dedicated infrastructure`), `customModels`,
  `slaOnboarding` (`Custom SLA + onboarding`) and `dedicatedSupport`. The pricing comparison has a
  `dedicated` row (`PricingPage.jsx:30`). `LandingPage.jsx:725` and `:726` say dedicated
  infrastructure, custom anomaly models, dedicated support and a custom SLA. `public/index.md:57`
  says custom SLAs, and `en.json` lines 808 and 809 offer SLA, self hosting and private deployments.
  The Turkish strings carry the same claims.

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

- Closed, with one part open. **Four lists named the same MCP tools and nothing held them together.**
  `MCP_TOOLS` and the `mcp_handler` dispatch were kept in step by a line in the project notes saying to edit
  both. The protected thing is a caller's ability to call what `tools/list` told them exists, so the
  contract has two sides and each is blind to the other's gap: from the manifest you cannot see an arm
  the manifest omits, and from the dispatch you cannot see an entry with no arm.
  `every_advertised_mcp_tool_answers` calls every advertised name and fails only on -32601, so a tool
  whose backing service is down still counts as answering. `no_mcp_tool_answers_unadvertised` scans
  the dispatch for the other direction, on whitespace-stripped source, bounded by the dispatch's own
  two ends rather than by a function name, and refuses to conclude anything from fewer than seven
  arms. The project-notes instruction is gone: an instruction kept by memory beside a test that replaces
  it reads as though the test were optional.

  `mcp_public_tools_need_no_token` held three names by hand under a comment that said four, and
  `get_kp_forecast` was the one missing. Both directions now come from the manifest's own wording,
  with the counts asserted, and the mirror test was added because the public one alone would pass if
  every tool were public.

  `frontend/src/lib/useWebMCP.js` was deleted rather than tied to `webmcp-init.js`. It was imported by
  nothing, so the drift the project-notes line warned about was between a live file and a dead one.
  Deleting the copy beats automating the reminder to update it. WebMCP's `get_neo_close_approaches`
  is now `get_neo`, the name the backend uses and the one the manifest test covers; nothing referenced
  the old name outside the files changed here and the server card below.

  `frontend/public/.well-known/mcp/server-card.json` was a fifth list, published at `/.well-known/mcp/`
  and naming `https://astraeusio.com/mcp` as its transport. It advertised eleven tools where the
  endpoint serves seven. Four were fiction: `get_xray_flux`, `get_imf`, `get_dst_index` and
  `get_space_weather_alerts` had no arm in the dispatch and never had one. Two more were the same tools
  under stale names, `get_kp_index` for `get_current_kp` and `get_neo_close_approaches` for `get_neo`.
  An agent reading the card and calling any of the six got "unknown tool" from a name the site
  published. It was found by accident, while grepping for callers of a name being changed, after three
  passes over the tool lists had missed it: being outside the Rust tree is the whole reason it
  survived.

  The card now carries the manifest's seven names and descriptions verbatim, and
  `the_server_card_advertises_what_the_endpoint_serves` holds it there in both directions. It parses
  the card rather than scanning it. The sibling scans strip whitespace before matching because a
  wrapped line defeats a text search, which has cost two guards here; a JSON parser makes the question
  moot, since whitespace is not part of the document and no formatting of the file can hide an entry.
  It also asserts the transport still points at `/mcp`, because if the card is repointed the two lists
  stop being about one thing and the comparison would keep passing while meaning nothing.

  The rest of `.well-known` was checked for the same shape, since the card was found by accident and
  siblings were likely. `openapi.json` declares 23 paths and every one is mounted, a documented subset
  rather than a claim. The four `agent-skills` SKILL.md files name 14 endpoints and all 14 are mounted.
  Two findings, both recorded rather than taken:

- No ID. **`agent-skills/index.json` declares a sha256 for each SKILL.md and all four are wrong.**
  Not a line ending artefact: the files are LF on disk and neither the LF nor the CRLF hash matches
  any declared value. The index publishes an integrity claim that fails for every entry it makes, so
  a consumer that checks it rejects all four skills and one that does not check gains nothing. The fix
  is not to recompute the four by hand, which drifts again on the next edit, but to generate the block
  or assert it, the way the server card is asserted now.
- No ID. **`http-message-signatures-directory` publishes an Ed25519 public key that nothing uses.**
  `kid` is `astraeusio-bot-2026`. Nothing in `backend/` or `frontend/src/` signs an HTTP message; the
  only match for ed25519 in the tree is a transitive entry in `Cargo.lock`. So the site tells a crawler
  where to verify signatures it never produces. Whether to sign or to remove the directory is a product
  decision, and it should not be settled by whoever next tidies the folder.

- Closed. **The password and address rules were tested and their application was not.**
  `the_password_rule_is_the_same_wherever_a_password_is_set` and
  `an_address_that_cannot_be_one_is_refused` asserted the two validators behave, and no test asserted
  any handler called them. A rule tested but unapplied passes every test while the defect ships.

  Enumerating from `validate_password` and `validate_email` found four call sites and looked
  complete: `register` twice, `change_password`, `reset_password`. Enumerating instead from what
  writes a credential into `users`, back through `db_writer`, found five. The fifth was `oauth.rs`,
  which stored a provider-supplied address that never saw `validate_email`. A list of call sites is a
  list of places already covered, so it cannot contain the site that has none. The sharper form of the
  rule: enumerate from the thing being protected, not from the protection.

  The OAuth callback now applies the same rule, under its own `email_invalid` code rather than
  `oauth_failed`, so a provider returning something malformed is one log line to diagnose. An
  exemption declared in a test whose purpose is catching unguarded entry points is the line the next
  person adding a provider copies, which is why it was closed rather than recorded. Four behavioural
  tests hold the four handler sites, asserting the message and not only the status because each has
  another route to a 4xx. `no_credential_reaches_the_users_table_unvalidated` is the net for a sixth
  path and is the weaker guard: it reads text, and its first version found three of four because
  rustfmt spreads the oauth call over three lines, so it matches against whitespace-stripped source
  now. Eight mutations, all caught.

- Closed. **Both host checks enumerated components from the payload they were checking.**
  `component-check.sh` looped over the components `/api/health` returned and asked of each whether it
  was fresh. `poller-check.sh` looked a mapped component up among the ones it was returned, and its
  one check of the mapping itself ran only under `--selftest`. Neither could see a name stop being
  published, because the list each compared against was built from the answer it was checking. Same
  shape as the three above, and the only instance where the authoritative list lives outside the
  repository, which is why it needed a file rather than a constant.

  `component-baseline.sh` holds the expected set in `/var/lib/astraeusio-components-baseline`,
  compares against it, and never writes it: a check that rewrites its own baseline turns a component
  going quiet into the new normal on the next run, and its self test asserts the file is untouched on
  every path rather than trusting that. `component-check.sh --accept-components` is the only writer,
  refuses an unreachable endpoint, an unparseable payload and an empty component set, and is the
  documented way to clear the raise. `poller-check.sh` reads the same file to decide whose alarm a
  missing component is, so one removal sends one mail, and now validates `COMPONENT_OF` on every run.
  Eleven mutations, one per behaviour, all caught; the runbook has the accept command under "A
  component stops being published".

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

- No ID. **A closure is written one way, and declared.** The form is
  `**AUD-NNN closed YYYY-MM-DD**`, with the identifier inside the marker, and the same identifier and
  date are declared in `CLOSED` in `backend/tests/backlog_structure.rs`. The test compares the two in
  both directions, so a closure that is marked and not declared fails, and so does one declared and
  not marked.

  The identifier is inside the marker because a marker without one cannot be attributed by machine.
  `AUD-012`'s closure sits seventy three lines below its own bullet with other top level list items in
  between, so slicing a bullet and reading the closure inside it does not work. Four spellings were in
  use until 2026-10-05 and counting them by eye gave three different wrong answers in one day: a
  pattern that matched only the compact spelling saw three of seven, reading the first `Closed` line in
  a bullet read a plan sentence as a closure, and a `No ID` bullet's closure was credited to the
  finding above it.

  A partial closure does not use the form. `AUD-027` shipped one half and says so in words, because a
  word meaning finished should not sit on something that is not.

- No ID. **The advisory round, outcome 2026-11-25.** `cargo audit` exits 0 and reports no
  vulnerability. The gate is 3 of 17 failing: `frontend test` (AUD-066), `ml test` (AUD-065), and
  `advisories`, which is red on chrono alone and is why nothing was deployed.

  Twenty three vulnerabilities at the start, zero at the end, in eight lock commits and one code
  commit: rustls and rustls-webpki (5), the two webpki trust stores (2), quinn-proto and quinn-udp
  (6), openssl (1), url (1), tokio (informational), rustix (informational), the arrow family
  (informational), duckdb for zip (6 of 7), zip itself (the 7th), jsonwebtoken for crypto-bigint (1).
  Informational went 4 to 1.

  **zip took two steps, and the first was not enough.** `duckdb 1.10506.1` brought `zip 9.0.0`, which
  still carries `RUSTSEC-2026-0548`, whose floor is 9.0.5. Taking zip to 9.4.1 needed `libc` 0.2.191,
  `nix` 0.31.3 and `typed-path` 0.13.0 to move with it, because `--precise` moves one package and
  refuses rather than resolving. 9.8.2 is available and needs `zeroize` 1.9, which nothing asked for.

  **jsonwebtoken 12 is a breaking change at six call sites.** `EncodingKey::from_secret` and
  `DecodingKey::from_secret` now return `Result`, failing only on an empty secret. Each site got the
  error channel it has rather than a blanket `?`: `purpose_token`, `session_jwt` and `sign_state`
  propagate, `decode_purpose` returns a distinct "server token key is unusable" rather than reusing
  the token message, the session extractor returns 500 rather than 401 because an unusable key is the
  server's fault, and `verify_state` logs and returns `None` because it has no error channel. Every
  one of those branches is unreachable while `require_jwt_secret` refuses a blank secret before the
  store opens, which is AUD-053 still paying for itself.

  **Nine tests and seven mutations.** Three mint and verify pairs, enumerated by reading every
  `encode` and `decode` in `auth.rs` and `oauth.rs`, each covered against a wrong secret, a wrong
  algorithm and an expired token. The algorithm case uses HS512 with the correct secret, so only the
  algorithm check can refuse it, and an `alg: none` token assembled from a valid one, whose header
  literal was decoded back to `{"alg":"none"}` before being written down. All seven mutations were
  caught by the test declared for each.

  **Two of the mutations as first written did not compile, and that is worth keeping.** They disabled
  signature validation, which jsonwebtoken 12 has no way to do: no
  `insecure_disable_signature_validation` and no `validate_signature` field. An unbuildable mutation
  proves nothing, so the wrong-secret pair was unexercised until it was replaced by a key built from
  the wrong source, which is the shape this codebase already had in AUD-053.

  **The production database opens under DuckDB 1.5.6, measured 2026-11-25 on a copy.**
  `astraeus_20261125.duckdb`, the 03:00 backup, 147,337,216 bytes, was copied from the host and opened
  with the 1.10506.1 build through the real binary rather than a probe: `DB_PATH` pointed at the copy,
  every poll interval was set to a day so the open was measured and not a fetch storm, and the reads
  went through the actual handlers.

  Twenty five authenticated read paths returned 200 and the log held no error or panic line. Three of
  them returned an empty array, and the code agrees with itself about why: `/api/health` reports
  `noaa_solar_wind`, `noaa_imf` and `noaa_xray` as degraded on this file while `noaa_kp`, `noaa_kp_3h`,
  `noaa_dst` and `iss` are operational, so `series_is_current` is refusing a series whose newest row is
  two hours old in a file frozen at 03:00. The rows themselves read correctly:
  `/api/reports/solar-wind`, which carries no freshness guard, returned 73,809 bytes from the same
  table. An earlier attempt measured sixteen 401s and proved only that the auth layer works, which is
  why the second run minted a token.

  **The engine writes to the file and does not upgrade its format.** The copy's sha256 changed from
  `3497e0e6...` to `79fdcde3...` with a 7,827 byte `.wal` beside it, so opening is not read only:
  migrations and the retention trim both write. The first 64 bytes are byte identical before and
  after, including the `version_number` field at `0x40` and the version string `v1.5.2`. So 1.5.6
  leaves a 1.5.2 file declaring 1.5.2, which is what `deploy.sh --rollback` would hand back to the
  previous image.

  That is inference from the file's own declared version, not a demonstration: no 1.5.2 build was run
  against the written copy, because that meant another bundled DuckDB compile on a disk at 12 GB. The
  header is the evidence, and it is the field the engine uses to decide whether it can open a file.

  **The `RUSTSEC-2023-0071` ignore is gone rather than rewritten.** jsonwebtoken 12 requires
  `rsa ^0.10` and the advisory is patched from 0.10.0, so the lockfile carries rsa 0.10.0 and
  `cargo audit` is silent about it with no entry telling it to be. Measured by removing the entry and
  running the tool, not by reading the comment. The reachability argument it rested on now lives in
  the nine tests. The rkyv entry stays, undecided, under AUD-069.

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
- No ID. **The order of work, decided 2026-11-24.** Five items, in sequence, one round each.

  1. The advisories, AUD-011.
  2. The two other red gate steps, AUD-065 and AUD-066.
  3. The swallowed errors from the AUD-052 hunt, AUD-060 first, then AUD-057, AUD-058, AUD-059,
     AUD-061 and AUD-062.
  4. Why the UptimeRobot alert for the 2026-10-27 outage never reached the owner, AUD-064.
  5. Remove caddy from the host, per the decision recorded under Operational.

  The sequence is not arbitrary. The advisories come first because `cargo audit` exiting non-zero
  blocks the next phase by the standing rules, and AUD-011 exists because that noise was tolerated
  once already. The two dead gate steps come next because a gate with three red steps trains
  everyone to read red as normal, and a step that cannot run is worse than one that fails. The
  swallowed errors come after, `poller.rs:803` first because it is the only one of the six that is
  silent in both directions: no alert and no log line.

  Written down because an order agreed in conversation and not written is an order that the next
  session reconstructs from scratch, and because items 4 and 5 are host work that no gate can
  prompt.

- No ID. **The advisory round, decisions of 2026-11-24.** Taken by the owner after the measurement in
  the report of the same day, and written here because the reasoning for four of the five is a cost
  that a commit message cannot carry.

  **1. Disk.** `cargo clean -p libduckdb-sys` is authorised before any lock change that will rebuild
  DuckDB, so orphaned build outputs never accumulate, and free space on C: is measured before and
  after each clean. Nothing else in `target` or anywhere else is deleted without naming it first. The
  first clean removed 23,429 files and 39.1 GiB, taking C: from 1.5 GB free to 31 GB.

  Why it is needed at all: a gate run whose build script fingerprint changed compiles the bundled
  amalgamation **twice**, once under clippy and once under `cargo test`, because the two produce
  different fingerprints. Each output is about 4 GB and the previous pair is orphaned rather than
  replaced. Sixteen such directories had reached 31.2 GB in a 55.3 GB `target`, and the `url` gate run
  failed on `LNK1180: insufficient disk space to complete link` rather than on anything in the code.

  **2. The gate runs after each upgrade, not once at the end.** Proposed as one run to save rebuilds
  and refused: the clean in item 1 is what makes per-upgrade gating affordable, so a regression still
  points at one crate.

  **3. zip, seven advisories including the 9.1 critical.** Take `duckdb 1.10506.1` and pin it
  `~1.10506` in `backend/Cargo.toml`. `libduckdb-sys 1.10506.1` requires `zip ^9`, which is the fix,
  and the tilde keeps the engine where it is. See AUD-068 for what the old `"1"` allowed.

  **The version scheme was read wrong in the report that preceded this decision, and the correction
  belongs here rather than only in conversation.** The second semver component was treated as a flat
  number, so `1.10502.0` was described as DuckDB 1.1.x and `1.20001.0` as 1.2.x. The duckdb-rs README
  states at line 137 that from DuckDB `v1.5.0` the format is `1.MAJOR_MINOR_PATCH.x`. The locked
  engine is **1.5.2**, the one being taken is **1.5.6**, and the one `"1"` would have allowed is
  **2.0.1**. A patch level engine move was described as a minor one and a major engine move as a minor
  one, which is exactly the misreading that would have made `cargo update` look harmless.

  **4. crypto-bigint, one advisory.** Bump `jsonwebtoken` from 10 to 12 rather than chase the crate.

  **Corrected 2026-11-25: the crate did not leave the tree.** The decision was taken on the reading
  that `elliptic-curve 0.14` has no crypto-bigint dependency, which the index does show. The resolve
  brought crypto-bigint in anyway, at **0.7.11**, through the rest of the 0.14 crypto stack. The
  advisory is cleared because 0.7.11 is past the patched floor of 0.7.6 and is not yanked, not because
  the dependency is gone. A fix by upgrade, then, and not the removal the decision described. The advisory's own fix target 0.7.6 is yanked, as are 0.7.3
  through 0.7.8. Every mint and verify site in `auth.rs` and `oauth.rs` gets a test that fails on a
  wrong secret, a wrong algorithm and an expired token, enumerated from the code rather than from the
  names. The `RUSTSEC-2023-0071` entry in `backend/.cargo/audit.toml` is re-measured under 12 and
  rewritten or removed to match.

  **5. chrono, unmaintained, no patched version. No decision yet, measurement only.** Asked whether
  duckdb can be built with arrow disabled so chrono leaves the tree. It cannot.
  `duckdb-1.10502.0/Cargo.toml` declares `[dependencies.arrow]` at version 58 with no
  `optional = true`, so no feature combination removes it, and this crate already builds at the
  minimum: duckdb's `default` feature set is empty and ours is `["bundled"]`. duckdb's own `chrono`
  dependency is optional, and it is off.
  `cargo tree -i chrono --target all -e normal` reports chrono reached through `arrow-arith`,
  `arrow-array`, `arrow-cast` and the rest of the arrow family under `arrow 58.1.0`, plus this crate
  directly. arrow 60.2.0, the newest, still requires `chrono ^0.4.40`, so upgrading arrow does not
  help either. The remaining paths are a fork of arrow, replacing DuckDB, or upstream dropping it.


- No ID. **Decisions of 2026-11-25.** Taken by the owner after the advisory round report of the same
  day. Each one is here because the reasoning is a cost or a rule change that a commit message
  cannot carry.

  **1. The deploy rule changes, and this round deploys.** An informational advisory that is already
  present in the production build does not block a deploy. A vulnerability advisory still blocks, and
  so does an informational advisory that a change introduces.

  The reason is what the old rule cost on 2026-11-24. Nine commits took twenty three vulnerabilities
  to zero, and the `advisories` step stayed red on chrono, which is unmaintained with no patched
  version and no way out while duckdb is a dependency. The rule as written held the rustls,
  quinn-proto, zip and jsonwebtoken fixes off production while changing nothing at all about chrono:
  the running build already carried chrono, and carried it before the round started. So the rule
  protected nothing and kept known, fixed vulnerabilities live on the server. The new wording keeps
  the part that matters, which is that a change may not introduce an advisory and may not leave a
  vulnerability unfixed.

  This does not relax the standing rule that no advisory is accepted or allowlisted. chrono stays
  open, stays counted, and stays reported red by the gate.

  Before deploying, a fresh backup through the existing backup path, confirmed landed. The new build
  writes the database under DuckDB 1.5.6, and a rollback must have a file the previous image can
  open. After deploying, the checks of the last deploy: exit code, restart counts, health, ERROR
  lines, 200 on `/`, `/api/health` and `/pricing`, and the AUD-050 CORS split.

  **2. chrono, measurement only, no change.** The owner searched RustSec and could not find a chrono
  unmaintained advisory, so the record must quote the source rather than describe it: the exact
  RUSTSEC id, its kind, the advisory text, any successor crate the advisory names, and the
  `cargo audit --json` entry it came from.

  **3. AUD-069, the rkyv ignore. Measurement before any change.** Whether rkyv is compiled into the
  shipped binary, which crate and feature pulls it into the lock, and whether any semver compatible
  version of that crate drops rkyv 0.7 or moves to 0.8.17 or later. A clean path is taken in one
  commit with a gate after it and the ignore removed. No clean path means nothing changes and the
  paths are reported with their cost.

  **4. AUD-070, the two clock dependent uptime tests.** The uptime handler gets an injectable clock,
  so the tests choose their day and keep their database coverage. The alternative, dropping to a pure
  unit test over a fabricated row set, was refused because it would lose the coverage the failure
  exposed. Every clock read in the handler is enumerated from the code and takes the injected clock.
  Tests cover the midnight crossing on both sides plus each existing case, and the mutation round
  names the killing assertion for each mutant.

  **5. Deletions, authorised by name.** The local copy of the production database in the session
  scratchpad, `prod_copy.duckdb` and its `.wal`, which holds real accounts, and the prunable worktree
  registration left by an earlier session, through `git worktree prune`. Nothing else.

  **Disk.** C: stood at 11.8 GB free when these were taken. `cargo clean -p libduckdb-sys` continues
  before any rebuild, and work stops with a report if free space falls under 5 GB.

- No ID. **Decisions of 2026-11-25, the second set.** Taken by the owner after the measurement
  report of the same day, so they answer what the first set asked to be measured.

  **1. Deploy the three commits the gate tested.** `6ff464e`, `94b2ad4` and `926762c`, so production
  runs what was measured rather than the commit before them. A fresh backup through `backup.sh`
  first, then the post deploy checks of the last deploy, each one reported.

  **2. chrono, now decided rather than measured.** Four parts.

  Our own code moves from chrono to jiff. That is the part of this advisory that sits at our source,
  and it is the next code round, not this one. Measured today: `chrono` is a direct dependency in
  `backend/Cargo.toml` line 23 and appears 30 times across seven files in `backend/src`, which
  `grep -rn chrono --include=*.rs src/` lists as `api_keys.rs`, `auth.rs`, `db.rs`, `oauth.rs`,
  `poller.rs`, `rate_limit.rs` and `routes.rs`.

  DuckDB is not replaced for an unmaintained notice with no vulnerability. The cost of replacing the
  storage engine is the whole schema, every query, the scaled integer encoding and the backup path,
  against a notice that says the crate is no longer maintained and names no defect. If a vulnerability
  advisory is ever published against chrono, this decision reopens.

  The `advisories` step stays red on chrono until arrow drops it. This is a known red step the owner
  has accepted, and the reason is that the dependency is not ours to move: `cargo tree -i chrono -e
  normal --target all` gives four paths, our own crate plus `arrow-arith`, `arrow-array` and
  `arrow-cast` at 58.4.2, all three under `arrow` under `duckdb` 1.10506.1. Moving our own code to
  jiff therefore shortens the list by one path and does not clear the advisory. Under the deploy rule
  taken earlier today it does not block a deploy, because the advisory is informational and was
  already in the production build.

  Upstream, asked for and found rather than opened. **apache/arrow-rs#9183, `Replace chrono w/ jiff`,
  open, filed 2026-01-15 by crepererum**, https://github.com/apache/arrow-rs/issues/9183. It quotes
  the chrono maintainer's own announcement of winding down maintenance, and a comment of 2026-11-18
  by RemiBardon names `RUSTSEC-2026-0543` directly, so the advisory and the upstream work are already
  joined there. The thread's current state, from comments of 2026-11-17 to 2026-11-23: the jiff work
  is being taken crate by crate behind an opt in feature, a public API survey found chrono types in
  `arrow-array`'s `temporal_conversions` among others, and switching the default and removing chrono
  are stated as separate later decisions. So there is no date to wait for. No issue was opened, as
  instructed.

  One nearby issue that does not help us, recorded so it is not mistaken for a fix:
  **apache/arrow-rs#12343**, `parquet: drop the required chrono dependency`, closed as completed
  2026-11-23 by PR #12345. It removes chrono from `parquet` only, and `cargo tree -i parquet` answers
  that parquet is not in our graph at all.

  **3. The project moves from the Windows checkout at `C:/astraeus` to WSL after this round.** No
  build is started after the deploy, so the move is not racing a target directory being written. The
  closing report lists git status, unpushed commits and every untracked or ignored file in the
  checkout outside `target/` and `node_modules/` with its size, so the owner can decide what moves.

  **The Windows checkout was deleted on 2026-11-26 by the owner**, after the first deploy from WSL,
  `db9cbba` to `c4f5efc`, succeeded. Before deleting it, the owner's final check found no unpushed
  commits, no modified tracked files, and no file newer than the move. From WSL the same day,
  `/mnt/c/astraeus` no longer exists. The check itself was the owner's and is recorded here as
  reported. It could not be repeated once the checkout was gone.

- No ID. **chrono, the advisory quoted rather than described, measured 2026-11-25.** The owner
  searched RustSec for a chrono unmaintained advisory and could not find one, so this records the
  source.

  The identifier is **RUSTSEC-2026-0543**, kind **unmaintained**, which is informational and not a
  vulnerability. It has no patched version: `patched = []`. The crate in the lockfile is chrono
  0.4.44.

  The advisory file, `crates/chrono/RUSTSEC-2026-0543.md` in the RustSec advisory database, in full:

  ```toml
  [advisory]
  id = "RUSTSEC-2026-0543"
  package = "chrono"
  date = "2026-09-14"
  informational = "unmaintained"
  url = "https://github.com/chronotope/chrono/issues/1768"

  [versions]
  patched = []
  ```

  ```
  # chrono is unmaintained

  The chrono crate is unmaintained; all versions are affected.

  Recommended alternatives:

  - [jiff](https://crates.io/crates/jiff)
  - [time](https://crates.io/crates/time)
  ```

  So the advisory names two successors, jiff and time, and neither is reachable from here while the
  dependency comes through arrow under duckdb.

  The `cargo audit --json` entry it came from, trimmed to the fields that matter and otherwise
  verbatim:

  ```json
  "warnings": { "unmaintained": [ {
    "kind": "unmaintained",
    "package": { "name": "chrono", "version": "0.4.44",
      "source": "registry+https://github.com/rust-lang/crates.io-index" },
    "advisory": { "id": "RUSTSEC-2026-0543", "package": "chrono",
      "title": "chrono is unmaintained",
      "description": "The chrono crate is unmaintained; all versions are affected.\n\nRecommended alternatives:\n\n- [jiff](https://crates.io/crates/jiff)\n- [time](https://crates.io/crates/time)",
      "date": "2026-09-14", "informational": "unmaintained",
      "url": "https://github.com/chronotope/chrono/issues/1768", "withdrawn": null },
    "versions": { "patched": [], "unaffected": [] } } ] }
  ```

  `withdrawn` is null, so it is live rather than retracted.

  **Why a search may have missed it: it is eight days old in the database.** The local advisory
  database is a git clone, and
  `git log -1 -- crates/chrono/RUSTSEC-2026-0543.md` gives commit `233155b4`, author `djc`, dated
  **2026-11-17**, while the advisory's own `date` field says 2026-09-14. The database head used for
  this measurement is `10db6998`, 2026-11-24, with 1567 advisories. A file added to the database a
  week ago can be ahead of what the website shows, and the `date` field is the day the advisory
  describes rather than the day it was published.

  That is also why the gate started reporting chrono only in this round. Nothing about this crate's
  dependencies changed to pull it in: chrono has been in the lockfile all along, and the advisory
  arrived.

- No ID. **Decisions of 2026-11-26.** Taken by the owner after the report that finished the move to
  WSL, which found that the gate runs Rust through sccache on WSL because the owner's shell exports
  `RUSTC_WRAPPER=sccache`, while the gate's own comment says it does not.

  **1. sccache.** The gate stops inheriting `RUSTC_WRAPPER` from the shell and sets it itself, so it
  runs the same way on every machine. The direction is decided by a measurement on WSL, not by the
  Windows result, because the Windows refusal came from `cl.exe` taking many `.cpp` files in one call
  and Linux may compile them one per call. The measurement starts from an empty sccache cache with
  zeroed stats, cleans `libduckdb-sys`, and builds twice with `RUSTC_WRAPPER=sccache`, cleaning
  `libduckdb-sys` between. After each build it records the C and C++ hits and misses, the
  non-cacheable reasons, and the wall time. If the second DuckDB build hits the cache and is
  materially faster, the gate sets `RUSTC_WRAPPER=sccache`. Otherwise it unsets it. The comment in
  `gate.sh` is rewritten to carry the Linux numbers and keeps the Windows measurement as history. The
  `~/.bashrc` line is the owner's and is not edited.

  **2. AUD-066.** Find the cause of the `frontend test` failure under Node 24 and fix it at the
  source. The step must fail if the tests stop being discovered, so a run that finds zero tests is a
  failure. The test files are enumerated from the tree, and the count the runner reports must match.

  **3. The mutation that the AUD-065 closure did not run.** Plant defects in the `CLOSED` half of the
  backlog structure test, at least a closure marker for an identifier that is not declared and a
  declared identifier marked closed twice, and show that each is caught by its named assertion.

  **4. `frontend/public/.well-known/security.txt`.** Read it, check every field against RFC 9116
  (`Contact`, `Expires` in the future, `Canonical` matching the URL it is served at), and track it in
  its own commit. Compare it with the file the server serves and report any difference without
  changing the server.

  Order: this record, then 1, 2, 3 and 4. The gate runs through `gate.sh` after each change. Push
  when done, no deploy, and report whether anything in the round changes the shipped image.

  **5. The security.txt contact.** Decided later the same day. `Contact` in
  `frontend/public/.well-known/security.txt` is `mailto:hello@astraeusio.com`, so the public file
  does not tie this project to another domain. The owner confirms that the address receives mail.
  The change ships in the first deploy from WSL, which starts with a fresh backup through
  `backup.sh`. After it, the origin must answer `/.well-known/security.txt` with 200 and
  `Content-Type: text/plain; charset=utf-8`, the CDN is purged, and the same URL is checked through
  Cloudflare. The Windows checkout is deleted only if this deploy succeeds.

- No ID. **Decisions of 2026-11-26, the second set.** Taken by the owner after walking through the
  logged in site.

  **1. The APOD round**, recorded as AUD-074 with its measurement plan and its fix. Added to it: the
  live dashboard's explanation for 2026-11-26 is the video fallback sentence, while NASA's API now
  returns the real explanation for that date, which is the production evidence of the second defect.
  The validator must also reject that explanation text, and it is one of the fixtures.

  **2. Ten findings from the walk through**, each verified in the code and the live API before it was
  recorded, AUD-075 to AUD-084. Two could not be confirmed from code alone and say so: AUD-080 and
  the data half of AUD-081.

  **3. Order of rounds**, one round each, the gate after each, a push after each, and no deploy until
  the owner says: APOD first. Then AUD-075, AUD-076, AUD-077, AUD-078, AUD-079 and AUD-080. Then
  AUD-081 and AUD-082. Then AUD-083 and AUD-084. Then chrono to jiff, AUD-067, and the gate step for
  the `forbid(unsafe_code)` attribute, as already decided.

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
