#!/bin/bash
#
# Edits one row of the users table with the backend stopped, then brings it
# back. This exists because the procedure used to live in docs/RUNBOOK.md as a
# block to paste, and on 2026-09-04 a pasted version of it failed between the
# stop and the update and left the backend down for 26 seconds. A file is read
# once before it runs and cannot fail halfway through a line.
#
# UNDO. A real run prints the exact command that reverses it, built from the
# values read out of the database rather than from what the caller believed
# they were. If a run is interrupted before that line appears, the general form
# is:
#
#   ./user-edit.sh --email <address> --set "<column>=<previous value>"
#
# The backend is started again on every exit path, including a failure and an
# interrupt, which is the one property the pasted version did not have.
#
# Three things that are not obvious.
#
# DuckDB will not open the live file even read-only while the backend holds the
# lock, so every read of current values happens inside the stopped window
# rather than before it. That is why --dry-run cannot show live values: it
# reads the newest backup instead and says so, because a dry run that stopped
# the backend would not be one.
#
# The version is checked before the stop, since a newer DuckDB can bump the
# storage format and then the backend cannot reopen the file at all.
#
# --set is refused if it contains a semicolon. The WHERE clause is built here,
# and a second statement smuggled through the assignment list would escape it.
set -euo pipefail

COMPOSE=${ASTRAEUS_COMPOSE:-docker compose -f /opt/astraeusio/docker-compose.yml}
DUCKDB=${ASTRAEUS_DUCKDB:-/opt/astraeusio/duckdb}
WANT_VERSION=${ASTRAEUS_DUCKDB_VERSION:-v1.5.2}
BACKUP_DIR=${ASTRAEUS_BACKUP_DIR:-/opt/astraeusio/backups}
# Assigned in two steps: the default carries %{http_code}, whose brace would
# close a ${VAR:-default} expansion early and leave the rest of the line to be
# run as a command.
HEALTH_CMD=${ASTRAEUS_HEALTH_CMD:-}
[ -n "$HEALTH_CMD" ] || HEALTH_CMD='curl -sk -o /dev/null -w %{http_code} --resolve astraeusio.com:443:127.0.0.1 https://astraeusio.com/api/health'
NULL_SENTINEL=$'\x01NULL\x01'

EMAIL=""
ASSIGNMENTS=""
DRY_RUN=0

die() { echo "user-edit: $*" >&2; exit 1; }

usage() {
  cat >&2 <<'USAGE'
usage: user-edit.sh --email <address> --set "<col>=<value>[, <col>=<value>...]" [--dry-run]
       user-edit.sh --selftest

  --email     the single account to change; the WHERE clause is built from it
  --set       assignment list, no semicolons, no WHERE of its own
  --dry-run   validate and report, stop nothing and write nothing; current
              values are read from the newest backup, not from the live file
USAGE
  exit 2
}

read_csv() { "$DUCKDB" -readonly "$1" -csv -noheader -c "$2"; }

backend_up() {
  $COMPOSE start backend >/dev/null 2>&1 || true
  local code=""
  for _ in $(seq 1 90); do
    code=$($HEALTH_CMD 2>/dev/null || true)
    [ "$code" = "200" ] && break
    sleep 1
  done
  echo "$code"
}

# Registered immediately after the stop, so every path back out of this script
# goes through it: success, error, and interrupt alike.
restore_backend() {
  local code
  code=$(backend_up)
  local ms=$(( $(date +%s%3N) - STOPPED_AT ))
  if [ "$code" = "200" ]; then
    echo "  backend answering again after ${ms} ms"
  else
    echo "  BACKEND DID NOT COME BACK, health returned '${code:-nothing}' after ${ms} ms" >&2
    echo "  run: $COMPOSE start backend" >&2
  fi
}

quote_literal() {
  case "$1" in
    "$NULL_SENTINEL") printf 'NULL' ;;
    true|false|TRUE|FALSE) printf '%s' "$1" ;;
    *[!0-9-]*|"") printf "'%s'" "${1//\'/\'\'}" ;;
    *) printf '%s' "$1" ;;
  esac
}

# Prints the current values of COLS for EMAIL out of the given database file,
# and the undo command that would put them back.
report_before() {
  local file=$1 label=$2 before val undo="" i=0 c
  before=$(read_csv "$file" "SELECT $SELECT_LIST FROM users WHERE email = '$EMAIL';")
  echo "before ($label):"
  local IFS=,
  read -r -a vals <<<"$before"
  unset IFS
  for c in "${COLS[@]}"; do
    val=${vals[$i]:-$NULL_SENTINEL}
    printf '  %s = %s\n' "$c" "$(quote_literal "$val")"
    [ -n "$undo" ] && undo+=", "
    undo+="$c=$(quote_literal "$val")"
    i=$((i + 1))
  done
  echo "undo for this run:"
  echo "  ./user-edit.sh --email '$EMAIL' --set \"$undo\""
}

main() {
  [ -n "$EMAIL" ] || usage
  [ -n "$ASSIGNMENTS" ] || usage
  case "$ASSIGNMENTS" in *";"*) die "--set must not contain a semicolon" ;; esac
  case "$EMAIL" in *"'"*) die "--email must not contain a quote" ;; esac

  local version
  version=$("$DUCKDB" --version | awk '{print $1}')
  [ "$version" = "$WANT_VERSION" ] \
    || die "duckdb is $version, expected $WANT_VERSION; a newer build can bump the storage format and the backend will not reopen the file"

  DB=${ASTRAEUS_DB:-$(docker volume inspect astraeusio_data --format '{{.Mountpoint}}')/astraeus.duckdb}
  [ -f "$DB" ] || die "no database at $DB"

  # The columns being written, so the undo can carry their previous values.
  COLS=()
  local a
  local IFS=,
  for a in $ASSIGNMENTS; do
    a=${a%%=*}
    a=$(echo "$a" | tr -d '[:space:]')
    [ -n "$a" ] || die "empty column name in --set"
    COLS+=("$a")
  done
  unset IFS

  SELECT_LIST=""
  local c
  for c in "${COLS[@]}"; do
    [ -n "$SELECT_LIST" ] && SELECT_LIST+=", "
    SELECT_LIST+="CASE WHEN $c IS NULL THEN '$NULL_SENTINEL' ELSE CAST($c AS VARCHAR) END"
  done

  echo "statement:"
  echo "  UPDATE users SET $ASSIGNMENTS WHERE email = '$EMAIL';"

  if [ "$DRY_RUN" = "1" ]; then
    local newest
    newest=$(ls -t "$BACKUP_DIR"/*.duckdb 2>/dev/null | head -1 || true)
    if [ -n "$newest" ]; then
      report_before "$newest" "from $(basename "$newest"), not the live file"
    else
      echo "no backup in $BACKUP_DIR, so current values cannot be read without stopping the backend"
    fi
    echo "dry run, nothing stopped and nothing written"
    return 0
  fi

  $COMPOSE stop backend >/dev/null 2>&1 || true
  STOPPED_AT=$(date +%s%3N)
  trap restore_backend EXIT

  local n
  n=$(read_csv "$DB" "SELECT COUNT(*) FROM users WHERE email = '$EMAIL';")
  [ "$n" = "1" ] || die "expected exactly one row for $EMAIL, found $n"

  report_before "$DB" "live"

  "$DUCKDB" "$DB" -c "UPDATE users SET $ASSIGNMENTS WHERE email = '$EMAIL';" >/dev/null
  echo "after:"
  read_csv "$DB" "SELECT $SELECT_LIST FROM users WHERE email = '$EMAIL';" | sed 's/^/  /'
}

selftest() {
  # Not local: the EXIT trap fires after this function has returned, and a
  # local would be out of scope by then, which under set -u kills the cleanup
  # after every check has already passed.
  SELFTEST_DIR=$(mktemp -d)
  local d=$SELFTEST_DIR
  trap 'rm -rf "$SELFTEST_DIR"' EXIT
  local fails=0
  check() {
    if [ "$2" = "$3" ]; then
      echo "  ok    $1"
    else
      echo "  FAIL  $1: expected '$3', got '$2'"
      fails=$((fails + 1))
    fi
  }

  cat >"$d/compose" <<'STUB'
#!/bin/sh
echo "compose $*" >> "$CALL_LOG"
STUB
  cat >"$d/duckdb" <<'STUB'
#!/bin/sh
case "$*" in
  *--version*) echo "v1.5.2 abcdef"; exit 0 ;;
esac
echo "duckdb $*" >> "$CALL_LOG"
case "$*" in
  *"COUNT(*)"*) echo "$ROW_COUNT" ;;
  *UPDATE*) [ -n "${UPDATE_FAILS:-}" ] && exit 7; echo "" ;;
  *) echo "$SELECT_RESULT" ;;
esac
STUB
  chmod +x "$d/compose" "$d/duckdb"
  : >"$d/db"
  mkdir -p "$d/backups"

  export ASTRAEUS_COMPOSE="$d/compose"
  export ASTRAEUS_DUCKDB="$d/duckdb"
  export ASTRAEUS_DB="$d/db"
  export ASTRAEUS_BACKUP_DIR="$d/backups"
  export ASTRAEUS_HEALTH_CMD="echo 200"
  export CALL_LOG="$d/calls.log"
  export ROW_COUNT=1
  export SELECT_RESULT="free"

  local out code

  : >"$CALL_LOG"
  out=$("$0" --email a@b.c --set "plan='pro'" --dry-run 2>&1) || true
  check "dry run stops nothing" "$(grep -c '^compose ' "$CALL_LOG")" "0"
  check "dry run says it cannot read the live file with no backup" \
    "$(echo "$out" | grep -c 'cannot be read without stopping')" "1"

  : >"$CALL_LOG"
  : >"$d/backups/astraeus_20260904.duckdb"
  out=$("$0" --email a@b.c --set "plan='pro'" --dry-run 2>&1) || true
  check "dry run reads the newest backup and labels it" \
    "$(echo "$out" | grep -c 'astraeus_20260904.duckdb, not the live file')" "1"
  check "  and reports the undo from that value" \
    "$(echo "$out" | grep -c -- "--set \"plan='free'\"")" "1"

  set +e
  out=$("$0" --email a@b.c --set "plan='pro'; DROP TABLE users" --dry-run 2>&1); code=$?
  set -e
  check "a semicolon in --set is refused" "$code" "1"
  check "  and says why" "$(echo "$out" | grep -c 'must not contain a semicolon')" "1"

  export ASTRAEUS_DUCKDB_VERSION=v9.9.9
  : >"$CALL_LOG"
  set +e
  out=$("$0" --email a@b.c --set "plan='pro'" 2>&1); code=$?
  set -e
  check "a duckdb version mismatch is refused" "$code" "1"
  check "  before anything is stopped" "$(grep -c '^compose ' "$CALL_LOG")" "0"
  unset ASTRAEUS_DUCKDB_VERSION

  # DuckDB will not open the live file read-only while the backend holds it, so
  # every read has to land after the stop. Asserted on the call order rather
  # than on the reads existing, because reading in the wrong place is exactly
  # the bug this ordering fixes and it would pass any test that only counted.
  : >"$CALL_LOG"
  out=$("$0" --email a@b.c --set "plan='pro'" 2>&1) || true
  check "the first call of a real run is the stop" \
    "$(head -1 "$CALL_LOG")" "compose stop backend"
  check "  and no read happens before it" \
    "$(awk '/^duckdb /{print NR; exit}' "$CALL_LOG")" "2"

  ROW_COUNT=0
  : >"$CALL_LOG"
  set +e
  out=$("$0" --email nobody@b.c --set "plan='pro'" 2>&1); code=$?
  set -e
  check "an address with no row is refused" "$code" "1"
  check "  and the backend is started again anyway" \
    "$(grep -c '^compose start backend$' "$CALL_LOG")" "1"
  ROW_COUNT=1

  : >"$CALL_LOG"
  export UPDATE_FAILS=1
  set +e
  out=$("$0" --email a@b.c --set "plan='pro'" 2>&1); code=$?
  set -e
  unset UPDATE_FAILS
  check "a failed update exits non-zero" "$( [ "$code" -ne 0 ] && echo yes || echo no )" "yes"
  check "  and the backend is started again anyway" \
    "$(grep -c '^compose start backend$' "$CALL_LOG")" "1"

  # The check above is specific to the trap, since a failed update reaches no
  # other start. This one pins the other side: a run that succeeds must not
  # start the backend twice.
  : >"$CALL_LOG"
  out=$("$0" --email a@b.c --set "plan='pro'" 2>&1) || true
  check "a successful run stops and starts exactly once" \
    "$(grep -c '^compose stop backend$' "$CALL_LOG")/$(grep -c '^compose start backend$' "$CALL_LOG")" "1/1"

  echo
  if [ "$fails" = "0" ]; then
    echo "selftest: all checks passed"
  else
    echo "selftest: $fails check(s) failed" >&2
    return 1
  fi
}

while [ $# -gt 0 ]; do
  case "$1" in
    --email) EMAIL=${2:-}; shift 2 ;;
    --set) ASSIGNMENTS=${2:-}; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --selftest) selftest; exit $? ;;
    -h|--help) usage ;;
    *) die "unknown argument: $1" ;;
  esac
done

main
