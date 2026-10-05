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
  on 2026-09-01 found no embedded credential, every one reads from `.cloudflare-key`, `.r2-s3-key`
  or `backend/.env`, all of which stay untracked at 600. So this is not an exposure, it is a
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

  **Closed the other way on 2026-09-22, in `98282e0`.** The claim came off rather than the delay
  going in: `delay60` is gone from `lib/plans.js`, the delay and real-time rows are gone from the
  comparison table, and the eight keys behind them are gone from both locales. The product applies
  no delay and no longer says it does. Serving two versions of the same series remains an option
  nobody has chosen, not an outstanding defect.

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

  **Closed.** The update now reads
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

  **Closed 2026-10-05.** Deleted, with the constant, and the reasoning left in place where the next
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

  **Closed 2026-10-05**, commits `afe5b3e` for the fix and `beb4351` for the guard, split because
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

  **Closed 2026-10-05.** The DDL declares `threshold_scaled BIGINT`, nullable rather than NOT NULL so
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

  **Closed 2026-10-05**, commits `89a66a9` for the split and `316e6d1` for the guard.

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

  **Closed 2026-10-05**, commit `1b0ae08`. The sweep rides inside the commit that adds the step rather
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

- **AUD-052** Three counts in `Store::open` swallow a failing query, and each makes the code believe
  something different. None is the fail-open class AUD-049 was.

  Found by scanning `Store::open` for the shape AUD-049 had, rather than from a report. The function
  holds 29 `query_row` calls; 26 use `?` and these three do not. What decides whether each matters is
  what reads the value afterwards, not the `.unwrap_or` itself.

  | line | binding | what reads it | what a failing count makes the code believe |
  |---|---|---|---|
  | `db.rs:1418` | `expected` | `era_fix_is_verified(expected, updated, inconsistent)` at `db.rs:1452` | that the `kp_forecast` era fix touched rows it should not have. `expected` becomes 0, so `updated == expected` is false whenever real work happened, and it logs an error naming a mismatch that did not occur |
  | `db.rs:1730` | `before` | `error!(before, after, "xray rebuild did not preserve every row")` at `db.rs:1752` | that the `xray` rebuild lost every row. `before` becomes 0 and the comparison against `after` reports a rebuild that destroyed data |
  | `db.rs:1691` | `carried` | one `info!` at `db.rs:1711` and nothing else | nothing. No decision reads it. The log line claims 0 rows were carried when rows were, so only the record is wrong |

  **Two of the three fail loud and one is cosmetic**, which is the opposite direction from AUD-049.
  There the swallowed error made a migration check read as "already done" and skip work in silence.
  Here a swallowed error raises a false alarm about data loss, or writes a misleading log. A false
  alarm about a rebuild destroying rows is still expensive, because the next person reads it as real.

  Closed by `?` at all three, which is what `every_migration_decision_propagates_its_query_error`
  already requires of the decision bindings and does not yet require of these.

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
  10 at `poller.rs:668` and 5 at `routes.rs:774`. One variable, two meanings depending on the path. Widening that test to
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

  **There is no live exposure**, and the entry says so rather than implying one. With `JWT_SECRET`
  unset the process reaches `main.rs:136` and panics, so it never serves. The defect is that a
  security guard's effectiveness depends on a variable checked later, in a different file, by a
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

  **Closed 2026-10-04.** The computation moved out of the handler into `uptime_report(now, interval,
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
  the live response on 2026-09-04. **Closed**: it now ships as
  `strict-origin-when-cross-origin` in `security-headers.conf`, so the token in a verification or
  reset link cannot cross an origin boundary. What remains open under this ID is the report-only
  CSP above, which is a separate deferral.

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
