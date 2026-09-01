#!/bin/bash
# Daily local backup of the live DuckDB database.
#
# The backend holds the database open and DuckDB refuses any second process
# open, even read only, so ATTACH and COPY FROM DATABASE are not available
# without stopping the container. The only option is a filesystem copy.
#
# A copy of the database alone loses everything written since the last
# checkpoint, which has been as much as fifteen hours. So the wal is copied
# too, then folded into the copy by opening it once. The stored backup is a
# single self contained file that needs no wal to restore.
#
# The database file only changes when a checkpoint runs. If one runs between
# copying the database and copying the wal, the wal has been truncated and the
# pair is inconsistent, so the database mtime is checked and the copy retried.
set -euo pipefail

# A backup is the whole database: every user row, bcrypt hash, API key hash and
# encrypted TOTP secret. These were being written 644. Created 600 from the
# start rather than tightened afterwards, so there is no window where the file
# exists and is readable by anyone else.
umask 077

SRC_DIR=/var/lib/docker/volumes/astraeusio_data/_data
SRC="$SRC_DIR/astraeus.duckdb"
WAL="$SRC.wal"
DST_DIR=/opt/astraeusio/backups
DUCKDB=/opt/astraeusio/duckdb
KEEP=7
ATTEMPTS=3
# Tables that must be present. A count was a proxy for this and went stale six
# minutes after it was written, when schema_migrations was added. Extras are
# ignored on purpose, so a new table never breaks the backup; what matters is
# that nothing expected has gone missing.
REQUIRED_TABLES="alerts_anomaly api_keys apod custom_anomaly_rules dst \
email_alerts epic exoplanet health_snapshots imf iss_position kp kp_3h \
kp_forecast neo schema_migrations solar_wind space_weather_alert starlink \
usage_records users webhook_deliveries webhooks xray"

DST="$DST_DIR/astraeus_$(date +%Y%m%d).duckdb"
TMP="$DST.partial"

log() { echo "$(date -u): $*"; }
fail() { echo "$(date -u): BACKUP FAILED: $*" >&2; exit 1; }

[ -f "$SRC" ] || fail "source database not found at $SRC"
[ -x "$DUCKDB" ] || fail "duckdb cli not found at $DUCKDB"
mkdir -p "$DST_DIR"

copied=0
for attempt in $(seq 1 "$ATTEMPTS"); do
  rm -f "$TMP" "$TMP.wal"
  before=$(stat -c %Y "$SRC")
  cp "$SRC" "$TMP"
  if [ -f "$WAL" ]; then
    cp "$WAL" "$TMP.wal"
  fi
  after=$(stat -c %Y "$SRC")
  if [ "$before" = "$after" ]; then
    copied=1
    break
  fi
  log "checkpoint ran during copy, attempt $attempt of $ATTEMPTS, retrying"
done
[ "$copied" = "1" ] || fail "database kept changing during copy after $ATTEMPTS attempts"

# Explicit as well as the umask, so an inherited umask cannot loosen it.
chmod 600 "$TMP"

# Fold the wal in. Opening replays it, and the checkpoint merges it, after
# which the wal file is no longer needed.
if ! "$DUCKDB" "$TMP" -c "CHECKPOINT;" > /dev/null 2>"$TMP.err"; then
  fail "copy does not open: $(head -c 300 "$TMP.err")"
fi
rm -f "$TMP.err" "$TMP.wal"

present=$("$DUCKDB" -readonly "$TMP" -csv -noheader \
  -c "SELECT table_name FROM information_schema.tables WHERE table_schema='main'" | tr -d '\r')
[ -n "$present" ] || fail "could not list tables in the copy"
missing=""
for t in $REQUIRED_TABLES; do
  grep -qx "$t" <<< "$present" || missing="$missing $t"
done
[ -z "$missing" ] || fail "required tables missing:$missing"
tables=$(echo "$present" | grep -c .)

counts=""
for t in kp kp_3h solar_wind xray imf dst iss_position; do
  c=$("$DUCKDB" -readonly "$TMP" -csv -noheader -c "SELECT count(*) FROM $t" | tr -cd '0-9')
  [ -n "$c" ] || fail "could not count rows in $t"
  [ "$c" -gt 0 ] || fail "table $t is empty"
  counts="$counts $t=$c"
done

newest=$("$DUCKDB" -readonly "$TMP" -csv -noheader -c "SELECT max(time_tag) FROM kp" | head -1)

mv "$TMP" "$DST"
log "backup -> $DST ($(du -h "$DST" | cut -f1)) tables=$tables newest_kp=$newest"
log "row counts:$counts"

# Keep the newest KEEP copies.
find "$DST_DIR" -name 'astraeus_*.duckdb' -type f | sort | head -n "-$KEEP" | xargs -r rm -f
find "$DST_DIR" -name 'astraeus_*.duckdb.partial*' -type f -mtime +1 -delete
