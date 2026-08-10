# Operations runbook

Procedures an operator runs by hand. Each one says what breaks, how to confirm
it, and what to do. Commands run on the production host unless stated.

The database lives in the Docker volume, not in the repository:

```
DBDIR=$(docker volume inspect astraeusio_data --format '{{.Mountpoint}}')
# $DBDIR/astraeus.duckdb
```

The backend holds that file open. Do not open it with a second writer. To read
it, copy the file and its `.wal` together and open the copy, which is what
`backup.sh` does.

---

## The backend refuses to start: TOTP_ENCRYPTION_KEY

`TOTP_ENCRYPTION_KEY` encrypts second factor secrets at rest. It is separate
from `JWT_SECRET` on purpose, and the backend refuses to start in three cases.

**Losing the key permanently destroys every enrolled second factor and forces
every 2FA user to re-enrol, and until an operator clears `totp_enabled` and
`totp_secret_enc` for them they cannot sign in at all.** The secrets themselves
are not recoverable by any means. The accounts are recoverable, by the procedure
below, at the cost of turning 2FA off for those users.

Read the error before acting; the three cases have different remedies.

### "TOTP_ENCRYPTION_KEY is not set but N account(s) have an encrypted second factor"

The key is missing from the environment. The secrets are intact.

1. Find the key. It belongs in `backend/.env` on the host. Check your password
   manager and any environment backup before concluding it is gone.
2. Put it back and start the stack. Nothing else is needed; no data was lost.

Only if the key is genuinely unrecoverable, follow **Clearing 2FA** below.

### "TOTP_ENCRYPTION_KEY does not decrypt the stored second factors"

The key present is not the key the secrets were encrypted with. This is usually
a copy paste error, or an environment restored from the wrong deployment.

Do not clear anything yet. Find the original key first. The stored secrets are
still valid and will work the moment the right key is supplied.

### "TOTP_ENCRYPTION_KEY must not be the same value as JWT_SECRET"

Generate a distinct key with `openssl rand -hex 32` and set it. If accounts are
already enrolled under the shared value, that shared value is the correct key
for them: set `TOTP_ENCRYPTION_KEY` to it temporarily is **not** possible,
because the check refuses. Clear 2FA for those accounts instead, set a fresh
distinct key, and have the users re-enrol.

### Clearing 2FA for locked out accounts

Last resort. This turns off 2FA for the named accounts and requires each user to
enrol again. It does not touch their password.

First, see who is affected. Work on a copy, never the live file:

```bash
DBDIR=$(docker volume inspect astraeusio_data --format '{{.Mountpoint}}')
cp "$DBDIR/astraeus.duckdb" /tmp/inspect.duckdb
cp "$DBDIR/astraeus.duckdb.wal" /tmp/inspect.duckdb.wal 2>/dev/null || true
/opt/astraeusio/duckdb /tmp/inspect.duckdb -c \
  "SELECT email, totp_enabled, totp_secret_enc IS NOT NULL AS has_secret
   FROM users WHERE totp_secret_enc IS NOT NULL OR totp_enabled;"
rm -f /tmp/inspect.duckdb /tmp/inspect.duckdb.wal
```

Then, with the stack stopped so nothing else holds the file:

```bash
cd /opt/astraeusio
./backup.sh                      # take a backup first, this is destructive
docker compose stop backend
/opt/astraeusio/duckdb "$DBDIR/astraeus.duckdb" -c \
  "UPDATE users SET totp_enabled = FALSE, totp_secret_enc = NULL
   WHERE email IN ('someone@example.com');"
docker compose start backend
```

Name the accounts explicitly. Do not run it without a `WHERE` clause.

Afterwards the backend starts, those users sign in with their password alone,
and each can enrol a new authenticator from Settings. Tell them; a second factor
disappearing without explanation looks like a compromise.

---

## The deploy verification accounts

Two permanent accounts exist only so a deploy can be verified end to end
against real sign ins and real tokens. Every deploy verification uses these
rather than anyone's personal account. Neither is email verified, which is
deliberate: nothing in the verification needs a delivered message, and neither
address has a mailbox.

They are split by tier because one account cannot prove both halves. The free
account proves the gate holds; the developer account proves the feature behind
the gate works. Asking one account to do both is what made the check
unsatisfiable the first time.

| Account | Tier | What it is for |
|---|---|---|
| `deploy-verify@astraeusio.com` | free | Sign in, token issue, failed sign in backoff, that the plan reads `free` after the starter retirement, and that key creation is correctly refused with `403 plan_required` |
| `deploy-verify-dev@astraeusio.com` | developer | The API key lifecycle: create with an expiry, the status fields, that the key authenticates, that revoking it returns 204, that the revoked key is then refused with 401, and that the row survives revocation as an audit trail |

Keep them at their tiers. The free one must stay free with no keys, no webhooks
and no custom rules, or it stops proving anything about the gate.

Credentials for both live in one file on the host,
`/opt/astraeusio/.deploy-verify-account`, mode 600, owned by root, untracked
like the other secrets beside it. They are not in the repository and must never
be printed into a log or a report.

```bash
CRED=/opt/astraeusio/.deploy-verify-account
# free account:      DEPLOY_VERIFY_EMAIL     / DEPLOY_VERIFY_PASSWORD
# developer account: DEPLOY_VERIFY_DEV_EMAIL / DEPLOY_VERIFY_DEV_PASSWORD
EMAIL=$(grep -E '^DEPLOY_VERIFY_EMAIL=' "$CRED" | cut -d= -f2-)
PASS=$(grep -E '^DEPLOY_VERIFY_PASSWORD=' "$CRED" | cut -d= -f2-)
curl -sSk -X POST -H 'Content-Type: application/json'   -d "{\"email\":\"$EMAIL\",\"password\":\"$PASS\"}" https://127.0.0.1/auth/login
```

Two things to know when using them. Six wrong passwords in a row put an account
into backoff and even the correct password is then refused until the wait
expires, so a verification run that ends in failures should wait it out rather
than retrying. And a successful sign in clears that counter, so ordinary use
leaves no trace.

If the credentials file is lost, delete the account rows from `users` and
create them again through `POST /auth/register`, then reapply the developer
tier with the procedure below; nothing depends on their identity.

### Still unverified: usage history

`GET /api/usage` returns a `history` array that is empty until a billing period
closes. It cannot be exercised on the day a deploy ships. Add it to the next
deploy verification instead of carrying it as proven, and only then treat the
history field as covered.

---

## Changing an account's plan

There is no operator path for this yet. Raising a tier through
`POST /api/user/plan` is refused unless `ALLOW_SELF_SERVE_PLAN_CHANGE` is set,
and that flag is deliberately off in production because with no payment
processor connected nothing can tell a paid tier from an unpaid one. Lowering a
tier is self serve and needs none of this.

So a raise is a database write, and a database write means stopping the
backend. DuckDB permits one read write process against the file; while the
backend holds it, a second process is refused, and in DuckDB 1.5.2 that is true
even for a read only open. There is no way to do this without a restart.

Two things to check before writing:

- The `duckdb` CLI on the host must be the same version the backend links, or a
  write can bump the storage format and the backend will not reopen the file.
  The backend uses `libduckdb-sys 1.10502.0`; that crate encodes the version as
  `1.MAJOR_MINOR_PATCH.x`, so it is DuckDB **1.5.2**. Confirm with
  `/opt/astraeusio/duckdb --version` and stop if it differs.
- Take a backup first if the account matters. This one did not, so this
  procedure did not.

```bash
EMAIL=deploy-verify-dev@astraeusio.com
DB=$(docker volume inspect astraeusio_data --format '{{.Mountpoint}}')/astraeus.duckdb

/opt/astraeusio/duckdb --version          # must read v1.5.2
docker compose -f /opt/astraeusio/docker-compose.yml stop backend
/opt/astraeusio/duckdb "$DB" -c "UPDATE users SET plan='developer' WHERE email='$EMAIL'; SELECT email, plan FROM users ORDER BY email;"
docker compose -f /opt/astraeusio/docker-compose.yml start backend
```

Measured downtime on 2026-08-10 was **705 ms** end to end, and the API answered
again on the second one second poll. Migrations do not re-run, since
`schema_migrations` already holds them.

The plan is also cached per account in the usage counter. Restarting the
backend clears that cache, so the new tier applies immediately; a write made
any other way would need `clear_user_cache` to be called or the value would
stay stale until the next restart.

When a payment processor is connected, its webhook becomes the only caller that
may raise a tier and this whole procedure goes away.

---

## Data caveats

Things that are true of the stored data and not visible from the schema.

### xray.satellite is unreliable before 2026-08

The `xray` table was keyed on `(time_tag, energy)` until the
`2026-08-xray-satellite-in-primary-key` migration. NOAA publishes from whichever
GOES satellite is currently primary, and at a switchover the same minute can be
republished under a new satellite number. Under the old key that collided with
the row already stored and `ON CONFLICT DO NOTHING` discarded the newcomer, so
which satellite is recorded against a row written before the migration depended
on which arrived first.

The flux values are unaffected: each row's `flux_e12` belongs with its own
`time_tag` and `energy`. Only the `satellite` label is untrustworthy, and only
for rows written before the migration.

To find the boundary date:

```bash
/opt/astraeusio/duckdb -readonly <copy> -c "
  SELECT applied_at, to_timestamp(applied_at) FROM schema_migrations
  WHERE id = '2026-08-xray-satellite-in-primary-key';"
```

Rows with `fetched_at` at or after that value have a trustworthy satellite.
Nothing in the product reads the column; it is returned by `/api/xray` and is
not rendered anywhere, so this matters only to an API consumer analysing by
spacecraft.

---

## Rolling back a deploy

Images are tagged with the commit they were built from, so a rollback is a
retag rather than a rebuild:

```bash
cd /opt/astraeusio
./deploy.sh --rollback <sha>
```

`docker images | grep astraeusio` lists the tags available on the host. If the
target tag is gone, rebuild from that commit instead:

```bash
git checkout <sha> -- ml/ backend/ frontend/
docker compose build
docker compose up -d
```

A rollback across a schema migration also needs the database restored from the
backup taken before the deploy, because the older binary does not know about the
newer columns. `backup.sh` writes to `/opt/astraeusio/backups/`.

---

## Backups

`backup.sh` runs at 03:00 and keeps 7 local copies. `backup-offsite.sh` runs at
03:30 and uploads to R2. `backup-check.sh` runs at 04:00 and 16:00 and alerts to
journald under tag `astraeusio-backup` if either is missing or stale.

To verify by hand:

```bash
/opt/astraeusio/backup-check.sh; echo "exit=$?"
journalctl -t astraeusio-backup --since '2 days ago'
```

Restoring is the reverse of the rollback procedure above: stop the backend, copy
the chosen backup over `astraeus.duckdb`, remove any `.wal` beside it, start.

---

## A data source has stopped

`poller-check.sh` runs hourly and alerts to journald under tag
`astraeusio-poller` when one source errors repeatedly.

```bash
/opt/astraeusio/poller-check.sh; echo "exit=$?"
journalctl -t astraeusio-poller --since '1 day ago'
docker logs --since 1h astraeusio-backend-1 2>&1 | grep ERROR
```

The status page at `/status` reports each series separately, so a single dead
feed is visible there rather than hidden behind a healthy neighbour. A series
past its freshness limit serves nothing rather than serving stale readings, so
an empty chart with a degraded status row is the expected appearance of a
stopped feed.
