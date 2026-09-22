#!/bin/bash
# Surfaces backup failure. Runs on its own schedule, not from the backup jobs,
# so a job that dies or never starts is still caught.
#
# Alerts when the local backup is missing or stale, when it is implausibly
# small, or when the off site copy is missing or stale. Alerting reuses the
# mail path the host already has; if that is absent it writes to the system log
# and to a state file, so the failure is visible either way.
#
# Exit status is 0 when everything is healthy and 1 when an alert fired, so it
# can also be called from a shell and read by a human.
set -uo pipefail

# Exit 10 means the check found a problem and has already notified. cron-run.sh
# treats it as reported and stays quiet, so one event produces one mail. Any
# other non-zero means the check itself broke, which cron-run.sh does mail,
# because at that point nothing else will.
EXIT_ALERT_SENT=10


# Overridable so --selftest can drive this against fixtures. Nothing in
# production sets them. Before this there was no way to exercise the check at
# all, so the floor below could not be tested against a size without waiting for
# a real backup to be that size.
BACKUP_DIR=${BACKUP_CHECK_DIR:-/opt/astraeusio/backups}
# The file backup.sh copies from. Must stay equal to SRC there, which
# --selftest asserts by reading both files.
LIVE_DB=${BACKUP_CHECK_LIVE_DB:-/var/lib/docker/volumes/astraeusio_data/_data/astraeus.duckdb}
R2_LATEST=${BACKUP_CHECK_R2_CMD:-python3 /opt/astraeusio/r2_latest.py}
# A new path, not the old one. The file this check used to write held
# "ok <epoch>" or "fail <epoch>" and was never read; alert-state.sh cannot tell
# that apart from a problem key, so on the first healthy run after the upgrade it
# read "ok 1788302718" as a problem that had just cleared and mailed a recovery
# for an outage that never happened. Changing the format changes the filename,
# which is cheaper than teaching one shared helper about one caller's history.
STATE=${BACKUP_CHECK_STATE:-/var/lib/astraeusio-backup-alert}
# The backup runs at 03:00 and this check at 04:00 and 16:00, so the oldest a
# healthy backup is ever seen is 13 hours, at the afternoon check. A missed
# nightly run shows as 25 hours at the next 04:00 check. 20 hours sits between
# the two with seven hours of margin either side, so a failed nightly is caught
# the same morning instead of a day later. The old value of 30 could not tell
# the two apart at all.
LOCAL_MAX_AGE_H=20
OFFSITE_MAX_AGE_H=20
# A backup is implausible when it is a fraction of the database it came from,
# not when it is below some number written down once.
#
# The old floor was an absolute 500 MB, and it was right for the four months it
# existed. /var/log/astraeusio-backup.log records the size of every run: 899M on
# 2026-08-10 rising steadily to 1.1G on 2026-09-01, all of it well clear of the
# floor. Then rebuild-db.sh compacted the database, the 2026-09-02 backup came
# out at 104M, and the same floor that had been correct the day before started
# calling every healthy backup too small. It mailed twice a day for nineteen
# days, 21 times, every one of them false.
#
# The defect is not the number. It is that compacting the database changed a
# property this check depended on and nothing connected the two, so the check
# went on measuring against a world that had stopped existing. An absolute floor
# has that failure built in: it cannot notice that the thing it measures has
# moved. A ratio against the live file cannot go stale the same way, because
# both sides move together.
#
# 50 percent, not 90. The failure this catches is a stub or a truncated copy,
# where the size collapses by an order of magnitude. DuckDB's file moves in both
# directions between checkpoints, and measured on 2026-09-21 the live file was
# 132,919,296 bytes against a backup of 133,705,728 taken thirteen hours
# earlier, so the backup was the larger of the two. A tight ratio would alarm on
# that ordinary behaviour.
MIN_SIZE_RATIO_PCT=50
# Delivery moved to notify.sh, which owns the recipient, the sender and
# the API key. Leaving these here would read as if they still controlled
# where alerts go, and they do not.

# Exercisable without alerting. NOTIFY_SEND=0 silences every script on this
# host at once, since all of them send through notify.sh; this is the local
# switch, for exercising this check alone. Both exist because neither was set
# when a harness ran poller-check.sh against fixture logs and mailed 37 alerts
# in eleven seconds. The harness isolated state and data correctly and did not
# isolate the effect the script exists to produce.
SEND_MAIL=${BACKUP_CHECK_MAIL:-1}

send_mail() {
  local subject=$1 body=$2
  if [ "$SEND_MAIL" != "1" ]; then
    echo "  (mail suppressed by BACKUP_CHECK_MAIL=$SEND_MAIL: $subject)"
    return 0
  fi
  /opt/astraeusio/notify.sh "$subject" "$body" 2>&1 | sed 's/^/  /'
}

if [ "${1:-}" = "--selftest" ]; then
  # shellcheck source=selftest-guard.sh
  . "$(dirname "$0")/selftest-guard.sh"
  require_notify_suppressed "backup-check.sh --selftest" || exit 2
  st_fail=0
  st_dir=$(mktemp -d)
  trap 'rm -rf "$st_dir"' EXIT
  st_check() { # what expected actual
    if [ "$2" = "$3" ]; then echo "  ok    $1"; else echo "  FAIL  $1: expected '$2', got '$3'"; st_fail=1; fi
  }

  echo "backup-check.sh --selftest"

  # A stub off site lister, so the selftest reaches no bucket.
  printf '#!/bin/sh\necho "astraeus_stub.gz 1"\n' > "$st_dir/r2"
  chmod +x "$st_dir/r2"

  # Runs this script against fixtures and echoes the problem kinds it found.
  st_run() { # live_bytes backup_bytes
    local d="$st_dir/run"
    rm -rf "$d"; mkdir -p "$d/backups"
    if [ "$1" != "none" ]; then
      head -c "$1" /dev/zero > "$d/live.duckdb"
    fi
    head -c "$2" /dev/zero > "$d/backups/astraeus_20260921.duckdb"
    BACKUP_CHECK_MAIL=0 \
    BACKUP_CHECK_DIR="$d/backups" \
    BACKUP_CHECK_LIVE_DB="$d/live.duckdb" \
    BACKUP_CHECK_R2_CMD="$st_dir/r2" \
    BACKUP_CHECK_STATE="$d/state" \
    bash "$0" 2>&1
  }

  out=$(st_run 1000000 1000000)
  st_check "a backup the size of the database is healthy" "0" "$(echo "$out" | grep -c 'local-small\|FAILED')"

  out=$(st_run 1000000 600000)
  st_check "sixty percent of the database is healthy" "0" "$(echo "$out" | grep -c 'is 600000 bytes')"

  out=$(st_run 1000000 100000); st_rc=$?
  st_check "ten percent of the database is too small" "1" "$(echo "$out" | grep -c 'under 50% of the live database')"
  # The exit status, not the wording. "backup check FAILED" appears twice when
  # mail is suppressed, once as the log line and once as the echoed subject, so
  # counting the phrase asserted the shape of the output rather than the result.
  st_check "  and the run exits with the alerted status" "$EXIT_ALERT_SENT" "$st_rc"

  # The real sizes from after the compaction. The old absolute floor called
  # these too small twice a day for nineteen days; a ratio does not.
  out=$(st_run 140000000 133705728)
  st_check "the real sizes from 2026-09-21 are healthy" "0" "$(echo "$out" | grep -c 'under 50%')"

  out=$(st_run none 1000000)
  st_check "an unreadable live database is a problem, not a silent pass" "1" "$(echo "$out" | grep -c 'cannot read the live database')"

  # One constant, two files. backup.sh copies from SRC and this sizes against
  # LIVE_DB; if they ever name different files this check measures the wrong one.
  st_src=$(grep -E '^SRC="' "$(dirname "$0")/backup.sh" | head -1)
  st_dirpart=$(grep -E '^SRC_DIR=' "$(dirname "$0")/backup.sh" | head -1 | cut -d= -f2)
  st_check "backup.sh still copies from the file this sizes against" \
    "1" "$(echo "$LIVE_DB" | grep -c "^$st_dirpart/astraeus.duckdb$")"
  st_check "  and backup.sh still builds SRC from SRC_DIR" "1" "$(echo "$st_src" | grep -c 'SRC_DIR')"

  echo
  if [ "$st_fail" = "0" ]; then echo "selftest passed"; else echo "SELFTEST FAILED"; fi
  exit "$st_fail"
fi

problems=()
# Stable identifiers for the same problems, for the alert key. The messages
# themselves carry ages that change every run.
kinds=()

# Local backup present, fresh, and a plausible size.
# shellcheck disable=SC2012  # names are generated by backup.sh as
# astraeus_YYYYMMDD.duckdb, so there is no whitespace or newline to mishandle,
# and ls -t is the clearest way to take the newest by mtime.
latest=$(ls -t "$BACKUP_DIR"/astraeus_*.duckdb 2>/dev/null | head -n 1 || true)
if [ -z "$latest" ]; then
  problems+=("no local backup found in $BACKUP_DIR")
  kinds+=("no-local")
else
  age_h=$(( ( $(date +%s) - $(stat -c %Y "$latest") ) / 3600 ))
  size=$(stat -c %s "$latest")
  if [ "$age_h" -gt "$LOCAL_MAX_AGE_H" ]; then
    problems+=("local backup $(basename "$latest") is ${age_h}h old, limit ${LOCAL_MAX_AGE_H}h")
    kinds+=("local-stale")
  fi
  # Compared against the live database rather than a constant. If that file
  # cannot be sized the comparison is impossible, and saying so is honest where
  # skipping the check silently would let a truncated backup through on exactly
  # the run that mattered.
  if [ ! -r "$LIVE_DB" ]; then
    problems+=("cannot read the live database at $LIVE_DB to size the backup against")
    kinds+=("live-db-unreadable")
  else
    live=$(stat -c %s "$LIVE_DB")
    if [ "$live" -le 0 ]; then
      problems+=("the live database at $LIVE_DB is empty")
      kinds+=("live-db-empty")
    elif [ $(( size * 100 / live )) -lt "$MIN_SIZE_RATIO_PCT" ]; then
      problems+=("local backup $(basename "$latest") is $size bytes, \
under ${MIN_SIZE_RATIO_PCT}% of the live database at $live bytes")
      kinds+=("local-small")
    fi
  fi
fi

# Off site copy present and fresh. Read only listing, object read permission.
offsite=$($R2_LATEST 2>/dev/null || true)
if [ -z "$offsite" ]; then
  problems+=("could not list the off site bucket")
  kinds+=("offsite-unreachable")
else
  off_key=$(echo "$offsite" | cut -d' ' -f1)
  off_age_h=$(echo "$offsite" | cut -d' ' -f2)
  if [ "$off_age_h" = "none" ]; then
    problems+=("no off site backup object found")
    kinds+=("offsite-missing")
  elif [ "$off_age_h" -gt "$OFFSITE_MAX_AGE_H" ]; then
    problems+=("off site $off_key is ${off_age_h}h old, limit ${OFFSITE_MAX_AGE_H}h")
    kinds+=("offsite-stale")
  fi
fi

mkdir -p "$(dirname "$STATE")"

# Transition mailing, which this check never had: it mailed on every failing run,
# twice a day, forever, for one stuck problem. The state file it has always
# written was never read, which is the tell that this was intended and not
# finished.
#
# The key is the kind of problem, not the message. The messages carry ages and
# filenames that change every run, so keying on them would mail every twelve
# hours exactly as before while looking like it had been fixed.
#
# shellcheck source=alert-state.sh
. "$(dirname "$0")/alert-state.sh"
current=$(printf '%s\n' ${kinds[@]+"${kinds[@]}"} | sort -u | tr '\n' ' ' | sed 's/ *$//')
alert_decide "$STATE" "$current"

if [ "${#problems[@]}" -eq 0 ]; then
  if [ "$ALERT_ACTION" = "recovered" ]; then
    line="Astraeusio backups recovered at $(date -u), after $ALERT_AGE_H. Previously: $ALERT_PREV"
    echo "$(date -u): backups recovered after $ALERT_AGE_H (was: $ALERT_PREV)"
    echo "$line" | logger -t astraeusio-backup -p daemon.notice
    send_mail "[RECOVERED after $ALERT_AGE_H] Astraeusio backups healthy" "$line" || true
  else
    echo "$(date -u): backup check ok"
  fi
  exit 0
fi

body="Astraeusio backup check failed at $(date -u)

$(printf '  %s\n' "${problems[@]}")

Local backups: $BACKUP_DIR
Host: $(hostname)"

echo "$(date -u): backup check FAILED"
printf '  %s\n' "${problems[@]}"

# Always log to the journal, which is retained and greppable.
echo "$body" | logger -t astraeusio-backup -p daemon.err

# Mail if the host has a working mailer. Not fatal if it does not. Once for a
# new problem, then again at six hours, a day, and daily after that.
case "$ALERT_ACTION" in
  new)
    send_mail "Astraeusio backup check FAILED" "$body" || true
    ;;
  escalate)
    send_mail "[STILL FAILING $ALERT_AGE_H] Astraeusio backup check" \
      "$body

Failing for $ALERT_AGE_H." || true
    ;;
  *)
    echo "  (already alerted for: $current, failing for $ALERT_AGE_H)"
    ;;
esac

exit "$EXIT_ALERT_SENT"
