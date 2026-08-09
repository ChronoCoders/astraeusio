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
