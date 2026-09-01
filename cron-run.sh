#!/bin/bash
# Runs a scheduled job and makes its failure visible on its own.
#
# The gap this closes: backup.sh exited 1 every night for a day and said exactly
# why, and nothing read it. The only monitor looked at how old the newest backup
# was, so the failure stayed invisible until its output aged past a threshold.
# A job that fails should be knowable because it failed, not because something
# it produces later becomes stale.
#
# Every cron entry runs through this. On a non zero exit it writes the job name,
# the exit code and the tail of the output to journald at daemon.err, keeps a
# per job state file, and mails if the host has a mailer. On success it clears
# the state file, so a job that recovers stops reporting without anyone acting.
#
# Usage: cron-run.sh <name> <command> [args...]
set -uo pipefail

NAME=${1:?usage: cron-run.sh <name> <command> [args...]}
shift
STATE_DIR=/var/lib/astraeusio-cron
LOG_DIR=/var/log
# Delivery moved to notify.sh, which owns the recipient, the sender and
# the API key. Leaving these here would read as if they still controlled
# where alerts go, and they do not.
TAIL_LINES=25

# Exercisable without alerting. NOTIFY_SEND=0 silences every script on this
# host at once, since all of them send through notify.sh; this is the local
# switch, for exercising this check alone. Both exist because neither was set
# when a harness ran poller-check.sh against fixture logs and mailed 37 alerts
# in eleven seconds. The harness isolated state and data correctly and did not
# isolate the effect the script exists to produce.
SEND_MAIL=${CRON_RUN_MAIL:-1}

send_mail() {
  local subject=$1 body=$2
  if [ "$SEND_MAIL" != "1" ]; then
    echo "  (mail suppressed by CRON_RUN_MAIL=$SEND_MAIL: $subject)"
    return 0
  fi
  /opt/astraeusio/notify.sh "$subject" "$body" 2>&1 | sed 's/^/  /'
}

mkdir -p "$STATE_DIR"
LOG="$LOG_DIR/astraeusio-$NAME.log"
OUT=$(mktemp "/tmp/cron-$NAME.XXXXXX")
trap 'rm -f "$OUT"' EXIT

start=$(date -u +%s)
"$@" > "$OUT" 2>&1
rc=$?
elapsed=$(( $(date -u +%s) - start ))

{
  echo "=== $(date -u) $NAME exit=$rc ${elapsed}s ==="
  cat "$OUT"
} >> "$LOG"

if [ "$rc" -eq 0 ]; then
  # Note a recovery once, then go quiet.
  if [ -f "$STATE_DIR/$NAME.failing" ]; then
    echo "astraeusio cron job $NAME recovered at $(date -u), exit 0" \
      | logger -t astraeusio-cron -p daemon.notice
    rm -f "$STATE_DIR/$NAME.failing"
  fi
  echo "ok $(date -u +%s)" > "$STATE_DIR/$NAME.last"
  exit 0
fi

# Exit 10 is the convention for a check that alerts for itself: it ran fine and
# found a problem it has already reported. Mailing again here is how one
# upstream outage became a mail every fifteen minutes. Log it, record it, and
# leave the telling to the job that knows what is actually wrong.
#
# Every other non-zero is the job breaking, which nobody else will report.
ALERT_SENT=10
if [ "$rc" -eq "$ALERT_SENT" ]; then
  echo "astraeusio cron job $NAME reported a problem and alerted for itself at $(date -u)" \
    | logger -t astraeusio-cron -p daemon.notice
  rm -f "$STATE_DIR/$NAME.failing"
  echo "alerted $(date -u +%s)" > "$STATE_DIR/$NAME.last"
  exit "$rc"
fi

body="Astraeusio cron job FAILED

job:      $NAME
command:  $*
exit:     $rc
when:     $(date -u)
duration: ${elapsed}s
host:     $(hostname)

last $TAIL_LINES lines of output:
$(tail -n "$TAIL_LINES" "$OUT")"

echo "$body" | logger -t astraeusio-cron -p daemon.err
echo "fail $(date -u +%s) exit=$rc" > "$STATE_DIR/$NAME.failing"
echo "fail $(date -u +%s)" > "$STATE_DIR/$NAME.last"

send_mail "Astraeusio cron FAILED: $NAME" "$body" || true

exit "$rc"
