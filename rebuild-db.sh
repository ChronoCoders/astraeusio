#!/bin/bash
# Rewrite the database into a fresh file, returning dead space to the disk.
#
# Deleting rows does not shrink a DuckDB file and neither does CHECKPOINT, which
# was measured: 1113 MB before, 1113 MB after, 0.03 seconds, because the freed
# space is reused inside the database rather than returned. The only thing that
# shrinks the file is writing a new one. On 2026-09-01 that took 1113 MB to
# 166 MB in three seconds, 85 percent of it dead space left by the hourly
# starlink replace, which is now an upsert.
#
# So this should rarely be needed. It exists for the one-off reclaim and for
# whatever the next write pattern turns out to be.
#
# By hand, not on cron. It stops the backend, and a procedure that does that
# should not fire unattended until it has been watched working a few times.
#
# The dangerous step is the swap, so the original is never deleted by this
# script:
#
#   1. it refuses to start without a recent backup
#   2. it stops the backend, because DuckDB allows one writer
#   3. it writes a NEW file beside the old one
#   4. it VERIFIES the new file opens, has every table, and has the same row
#      count in each, before anything is moved
#   5. only then does it move the original aside and the new one into place
#   6. if the backend does not come back healthy, it puts the original back
#
# A rebuild that produces a file which will not open therefore changes nothing:
# the new file is deleted, the original has not moved, and the backend restarts
# on it. The original is left on disk as .prerebuild afterwards for you to
# remove once you are satisfied, rather than being cleaned up automatically.
set -uo pipefail

VOLUME=${DB_VOLUME_DIR:-/var/lib/docker/volumes/astraeusio_data/_data}
DB="$VOLUME/astraeus.duckdb"
NEW="$VOLUME/astraeus.rebuild.duckdb"
OLD="$DB.prerebuild"
DUCKDB=${DUCKDB_BIN:-/opt/astraeusio/duckdb}
COMPOSE_DIR=${COMPOSE_DIR:-/opt/astraeusio}
BACKUP_DIR=${BACKUP_DIR:-/opt/astraeusio/backups}
MAX_BACKUP_AGE_HOURS=${MAX_BACKUP_AGE_HOURS:-30}

log()  { echo "$(date -u '+%H:%M:%S')  $*"; }
fail() { echo "$(date -u '+%H:%M:%S')  FAILED: $*" >&2; exit 1; }

mb() { echo $(( $(stat -c %s "$1") / 1048576 )); }

compose() { docker compose -f "$COMPOSE_DIR/docker-compose.yml" "$@"; }

# ── preflight ────────────────────────────────────────────────────────────────
[ -x "$DUCKDB" ] || fail "no duckdb cli at $DUCKDB"
[ -f "$DB" ]     || fail "no database at $DB"
[ -e "$NEW" ]    && fail "$NEW already exists; a previous run did not finish. Look at it first."
[ -e "$OLD" ]    && fail "$OLD already exists from an earlier rebuild. Remove or rename it first."

recent=$(find "$BACKUP_DIR" -name 'astraeus_*.duckdb' -mmin "-$((MAX_BACKUP_AGE_HOURS * 60))" | head -1)
[ -n "$recent" ] || fail "no backup newer than ${MAX_BACKUP_AGE_HOURS}h in $BACKUP_DIR. Run backup.sh first."
log "preflight"
log "  backup: $(basename "$recent")"

before=$(mb "$DB")
free=$(df --output=avail -m "$VOLUME" | tail -1)
[ "$free" -gt "$((before * 2))" ] || fail "need room for a second copy: ${before} MB database, ${free} MB free"
log "  database ${before} MB, ${free} MB free"

# ── stop the writer ──────────────────────────────────────────────────────────
log "stopping the backend"
compose stop backend >/dev/null 2>&1 || fail "could not stop the backend"

restore_and_start() {
  if [ -f "$OLD" ] && [ ! -f "$DB" ]; then
    mv "$OLD" "$DB"
    [ -f "$OLD.wal" ] && mv "$OLD.wal" "$DB.wal"
  fi
  rm -f "$NEW" "$NEW.wal"
  compose start backend >/dev/null 2>&1
}

# ── rebuild ──────────────────────────────────────────────────────────────────
log "rebuilding"
if ! "$DUCKDB" "$DB" -c "ATTACH '$NEW' AS fresh; COPY FROM DATABASE astraeus TO fresh; DETACH fresh;" >/dev/null 2>&1; then
  # The name a database gets when opened by path is the file's stem, so try the
  # stem too rather than guessing once.
  stem=$(basename "$DB" .duckdb)
  if ! "$DUCKDB" "$DB" -c "ATTACH '$NEW' AS fresh; COPY FROM DATABASE $stem TO fresh; DETACH fresh;" >/dev/null 2>&1; then
    restore_and_start
    fail "the rebuild did not run. Nothing was moved, the backend is back on the original."
  fi
fi
[ -f "$NEW" ] || { restore_and_start; fail "the rebuild produced no file. Nothing was moved."; }
log "  wrote $(mb "$NEW") MB"

# ── verify before anything moves ─────────────────────────────────────────────
log "verifying the new file"
tables_sql="SELECT table_name FROM duckdb_tables() ORDER BY table_name"
old_tables=$("$DUCKDB" -readonly "$DB" -noheader -list -c "$tables_sql" 2>/dev/null)
new_tables=$("$DUCKDB" -readonly "$NEW" -noheader -list -c "$tables_sql" 2>/dev/null) \
  || { restore_and_start; fail "the rebuilt file does not open. It has been deleted and the backend is back on the original."; }

if [ "$old_tables" != "$new_tables" ]; then
  restore_and_start
  fail "the rebuilt file has a different set of tables. It has been deleted and nothing was moved."
fi

mismatch=""
while read -r t; do
  [ -z "$t" ] && continue
  a=$("$DUCKDB" -readonly "$DB"  -noheader -list -c "SELECT count(*) FROM \"$t\"" 2>/dev/null)
  b=$("$DUCKDB" -readonly "$NEW" -noheader -list -c "SELECT count(*) FROM \"$t\"" 2>/dev/null)
  if [ "$a" != "$b" ]; then
    mismatch="$mismatch $t($a vs $b)"
  fi
done <<< "$old_tables"

if [ -n "$mismatch" ]; then
  restore_and_start
  fail "row counts differ:$mismatch. The rebuilt file has been deleted and nothing was moved."
fi
log "  $(echo "$old_tables" | grep -c .) tables, row counts match"

# ── swap ─────────────────────────────────────────────────────────────────────
# The WAL belongs to the file it was written for. Left behind, DuckDB would
# replay it onto the new file, which is a different database with the same name.
log "swapping"
mv "$DB" "$OLD" || fail "could not move the original aside. Nothing changed."
[ -f "$DB.wal" ] && mv "$DB.wal" "$OLD.wal"
mv "$NEW" "$DB" || { restore_and_start; fail "could not move the new file into place. The original is back."; }

log "starting the backend"
compose start backend >/dev/null 2>&1 || { restore_and_start; fail "the backend did not start. The original is back."; }

for _ in $(seq 1 60); do
  if compose exec -T backend curl -fsS --max-time 3 http://127.0.0.1:3000/health >/dev/null 2>&1; then
    log "rebuilt ${before} MB to $(mb "$DB") MB, backend healthy"
    log "the original is at $OLD. Remove it when you are satisfied."
    exit 0
  fi
  sleep 2
done

log "the backend did not become healthy, putting the original back"
compose stop backend >/dev/null 2>&1
mv "$DB" "$NEW"
mv "$OLD" "$DB"
[ -f "$OLD.wal" ] && mv "$OLD.wal" "$DB.wal"
compose start backend >/dev/null 2>&1
fail "the backend would not come up on the rebuilt file. The original is back in place and the rebuilt one is at $NEW for inspection."
