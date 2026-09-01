#!/bin/bash
# Surfaces a data source that has gone quiet without anything failing.
#
# The gap this closes: a poller can succeed and write nothing. Celestrak answers
# "no change" on half its polls, an upstream can return an empty payload, and a
# feed can change shape so every row is dropped. None of that raises an error,
# so poller-check.sh, which counts ERROR lines, cannot see it. A check that reads
# logs only sees what the process chose to say.
#
# So this one reads data instead, and asks one question: is every component as
# fresh as it is supposed to be. It does not carry its own thresholds. A stale
# limit in bash would become a second source of truth and drift from
# SERIES_FRESHNESS in db.rs, and it would be wrong in both directions, since
# kp_3h legitimately lags nine hours and starlink two. /api/health already
# applies those constants, so this asks the backend and trusts the answer.
#
# Division of labour with healthcheck.sh, which runs every five minutes:
#   healthcheck.sh  the site is unreachable            -> it alerts
#   this            the site answers, a component is stale -> this alerts
# If the endpoint cannot be reached or does not parse, this exits 0 without
# alerting and says it deferred. Two alarms for one outage is worse than one.
#
# Alerting goes through Resend because the host has no mail binary, so journald
# alerts are written and never read. The key is read from backend/.env rather
# than copied here; there is already one hardcoded copy in healthcheck.sh and a
# second would be one more place to rotate.
#
# Exit status is 0 when healthy or deferring and 1 when an alert fired, so it
# can be run by hand and read by a human.
set -uo pipefail

# Exit 10 means the check found a problem and has already notified. cron-run.sh
# treats it as reported and stays quiet, so one event produces one mail. Any
# other non-zero means the check itself broke, which cron-run.sh does mail,
# because at that point nothing else will.
EXIT_ALERT_SENT=10


# Reaches the origin through the internal listener on 8081, which is plaintext
# and asks for no client certificate. Until 2026-08-14 this went to 443 with
# --resolve, which succeeds only while ssl_verify_client is `optional`; the
# dependency was invisible and shared with three other callers. See the 8081
# server block in frontend/nginx.conf.
URL=${COMPONENT_CHECK_URL:-http://127.0.0.1:8081/api/health}
STATE=${COMPONENT_CHECK_STATE:-/var/lib/astraeusio-component-state}
# Delivery moved to notify.sh, which owns the recipient, the sender and
# the API key. Leaving these here would read as if they still controlled
# where alerts go, and they do not.
SEND_MAIL=${COMPONENT_CHECK_MAIL:-1}

send_mail() {
  local subject=$1 body=$2
  if [ "$SEND_MAIL" != "1" ]; then
    echo "  (mail suppressed by COMPONENT_CHECK_MAIL=$SEND_MAIL: $subject)"
    return 0
  fi
  # Delivery lives in notify.sh so there is one sender and one key.
  /opt/astraeusio/notify.sh "$subject" "$body" 2>&1 | sed 's/^/  /'
}

# ── Ask the backend ───────────────────────────────────────────────────────────

body=$(curl -s --max-time 15 -w '\n%{http_code}' "$URL" 2>/dev/null)
curl_rc=$?
http=$(printf '%s' "$body" | tail -n 1)
payload=$(printf '%s' "$body" | sed '$d')

if [ "$curl_rc" -ne 0 ] || [ "$http" != "200" ]; then
  echo "$(date -u): endpoint unreachable (curl=$curl_rc http=${http:-none}). Not alerting; healthcheck.sh owns this."
  exit 0
fi

parsed=$(printf '%s' "$payload" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
    comps = d["components"]
except Exception as e:
    print("PARSE_FAILED", e)
    sys.exit(0)
print("OK", d.get("status", "?"))
for name in sorted(comps):
    c = comps[name]
    st = c.get("status", "?") if isinstance(c, dict) else str(c)
    last = c.get("last_update", c.get("last_write", c.get("last_checked"))) if isinstance(c, dict) else None
    print(name, st, last if last is not None else "-")
' 2>/dev/null)

if [ -z "$parsed" ] || [ "${parsed%% *}" = "PARSE_FAILED" ]; then
  echo "$(date -u): /api/health did not parse (${parsed:-empty}). Not alerting; healthcheck.sh owns the endpoint."
  exit 0
fi

overall=$(printf '%s\n' "$parsed" | head -n 1 | awk '{print $2}')
now=$(date -u +%s)

bad=()
while read -r name status last; do
  [ -z "${name:-}" ] && continue
  if [ "$status" != "operational" ]; then
    if [ "$last" != "-" ] && [ -n "$last" ]; then
      age=$(( (now - last) / 60 ))
      bad+=("$name is $status, last update ${age}m ago")
    else
      bad+=("$name is $status, no recorded update")
    fi
  fi
done < <(printf '%s\n' "$parsed" | tail -n +2)

# ── Compare with what was already known ───────────────────────────────────────

mkdir -p "$(dirname "$STATE")"
# Component names only. The ages change every run, so keying the "already
# alerted" comparison on the full message would re-mail every time.
current=$(printf '%s\n' "${bad[@]:-}" | awk 'NF {print $1}' | sort | tr '\n' ' ' | sed 's/ *$//')

# Whether this is new, an escalation on age, or something already said. Shared
# with poller-check.sh and backup-check.sh, so the rule lives in one file with a
# self test rather than in three copies that drift.
# shellcheck source=alert-state.sh
. "$(dirname "$0")/alert-state.sh"
alert_decide "$STATE" "$current"

if [ "${#bad[@]}" -eq 0 ]; then
  if [ "$ALERT_ACTION" = "recovered" ]; then
    line="Astraeusio components recovered at $(date -u), after $ALERT_AGE_H. All components operational again. Previously: $ALERT_PREV"
    echo "$(date -u): recovered after $ALERT_AGE_H, all components operational (was: $ALERT_PREV)"
    echo "$line" | logger -t astraeusio-components -p daemon.notice
    send_mail "[RECOVERED after $ALERT_AGE_H] Astraeusio components healthy" "$line"
  else
    echo "$(date -u): component check ok (overall=$overall)"
  fi
  exit 0
fi

report="Astraeusio component check FAILED at $(date -u)

$(printf '  %s\n' "${bad[@]}")

overall: $overall
source:  $URL
host:    $(hostname)

A component is degraded when its newest observation is older than the limit the
backend keeps for that series. It means the poller is no longer writing, whether
or not anything logged an error."

echo "$(date -u): component check FAILED (overall=$overall)"
printf '  %s\n' "${bad[@]}"
echo "$report" | logger -t astraeusio-components -p daemon.err

# Once per distinct set of bad components, so one that stays degraded does not
# mail every run, and again on age at six hours, at a day, and daily after that.
# The seventeen hour outage on 2026-08-31 sent exactly one mail before this.
case "$ALERT_ACTION" in
  new)
    send_mail "[ALERT] Astraeusio components degraded: $current" "$report"
    ;;
  escalate)
    send_mail "[STILL DEGRADED $ALERT_AGE_H] Astraeusio components: $current" \
      "$report

Degraded for $ALERT_AGE_H and still failing."
    ;;
  *)
    echo "  (already alerted for: $current, degraded for $ALERT_AGE_H)"
    ;;
esac

exit "$EXIT_ALERT_SENT"
