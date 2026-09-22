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
# The set of components the backend is expected to publish, written only by
# `--accept-components`. Shared with poller-check.sh, which reads it to decide
# whose alarm a missing component is; one file so one acceptance settles it for
# both, rather than two that can disagree about what is normal.
BASELINE=${COMPONENT_CHECK_BASELINE:-/var/lib/astraeusio-components-baseline}
# How many consecutive runs a name must appear on before it is worth a mail.
#
# Two, not one. Over the 28 days to 2026-09-21 this check mailed 37 times and
# almost every incident recovered inside one or two cycles: 14m, 15m, 30m, 59m.
# One sample was enough to alert, and one sample is not evidence.
#
# The cost is that a real outage is announced one cycle later, 15 minutes here,
# and that the age alert-state.sh escalates on starts at confirmation rather
# than first sight, so ages read one cycle short. Nothing is suppressed: a
# problem that persists is still mailed, just not on its first sample.
CONFIRM_RUNS=${COMPONENT_CHECK_CONFIRM_RUNS:-2}
# Separate from STATE, which alert-state.sh owns and whose format it parses. A
# caller that shares a state file with that helper gets its own bookkeeping read
# back as a problem key, which is how a recovery mail once went out for an
# outage that never happened.
PENDING=${COMPONENT_CHECK_PENDING:-/var/lib/astraeusio-component-pending}

# Delivery moved to notify.sh, which owns the recipient, the sender and
# the API key. Leaving these here would read as if they still controlled
# where alerts go, and they do not.
SEND_MAIL=${COMPONENT_CHECK_MAIL:-1}

# Records the components the endpoint publishes right now as the expectation.
# Run by hand after a deploy that deliberately adds or removes one. Nothing on
# the cron path sets it, which is the point: see component-baseline.sh.
ACCEPT=0
[ "${1:-}" = "--accept-components" ] && ACCEPT=1

send_mail() {
  local subject=$1 body=$2
  if [ "$SEND_MAIL" != "1" ]; then
    echo "  (mail suppressed by COMPONENT_CHECK_MAIL=$SEND_MAIL: $subject)"
    return 0
  fi
  # Delivery lives in notify.sh so there is one sender and one key.
  /opt/astraeusio/notify.sh "$subject" "$body" 2>&1 | sed 's/^/  /'
}

if [ "${1:-}" = "--selftest" ]; then
  # shellcheck source=selftest-guard.sh
  . "$(dirname "$0")/selftest-guard.sh"
  require_notify_suppressed "component-check.sh --selftest" || exit 2
  export COMPONENT_CHECK_MAIL=0

  st_fail=0
  st_d=$(mktemp -d)
  st_srv=""
  cleanup() { [ -n "$st_srv" ] && kill "$st_srv" 2>/dev/null; rm -rf "$st_d"; }
  trap cleanup EXIT
  st_check() {
    if [ "$2" = "$3" ]; then echo "  ok    $1"; else echo "  FAIL  $1: expected '$2', got '$3'"; st_fail=1; fi
  }

  mkdir -p "$st_d/www"
  echo "backend_api database nasa_apod" > "$st_d/baseline"

  st_payload() { # status_for_apod age_secs
    local now; now=$(date -u +%s)
    cat > "$st_d/www/health" <<JSON
{"status":"$1","checked_at":$now,"components":{
 "backend_api":{"status":"operational","last_checked":$now},
 "database":{"status":"operational","last_write":$now},
 "nasa_apod":{"status":"$1","last_update":$(( now - $2 ))}}}
JSON
  }

  # The port comes from the kernel. A fixed one left a server from the previous
  # run holding it, the new server could not bind, every fetch returned 404, and
  # this script correctly declined to alert on an unreachable endpoint. Every
  # assertion failed and not one of them was about the code, which is why the
  # control below exists before any of them.
  cat > "$st_d/server.py" <<'SRVPY'
import http.server, socketserver, sys, os
os.chdir(sys.argv[1])
socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("127.0.0.1", 0), http.server.SimpleHTTPRequestHandler) as s:
    print(s.server_address[1], flush=True)
    s.serve_forever()
SRVPY

  st_payload degraded 100000
  python3 "$st_d/server.py" "$st_d/www" > "$st_d/port" 2>/dev/null &
  st_srv=$!
  st_port=""
  for _ in $(seq 1 40); do
    st_port=$(cat "$st_d/port" 2>/dev/null)
    [ -n "$st_port" ] && break
    sleep 0.25
  done
  if [ -z "$st_port" ]; then
    echo "the fixture server never reported a port; nothing below would mean anything"
    exit 1
  fi
  st_url="http://127.0.0.1:$st_port/health"

  st_run() {
    COMPONENT_CHECK_URL="$st_url" \
    COMPONENT_CHECK_STATE="$st_d/state" \
    COMPONENT_CHECK_PENDING="$st_d/pending" \
    COMPONENT_CHECK_BASELINE="$st_d/baseline" \
    bash "$0" 2>&1
    echo "RC=$?"
  }

  echo "component-check.sh --selftest (port $st_port)"
  echo
  echo "1. the fixture is really being served"
  st_check "the endpoint answers 200" "200" "$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$st_url")"
  st_check "  with our fixture, not somebody else's" "1" "$(curl -s --max-time 5 "$st_url" | grep -c nasa_apod)"

  echo
  echo "2. one bad sample then healthy, the flap that used to mail twice"
  out=$(st_run)
  st_check "a first bad sample does not alert" "0" "$(echo "$out" | grep -c 'suppressed by COMPONENT_CHECK_MAIL')"
  st_check "  and says what it is holding" "1" "$(echo "$out" | grep -c 'not yet confirmed, no alert: nasa_apod')"
  st_check "  and exits clean" "1" "$(echo "$out" | grep -c 'RC=0')"

  st_payload operational 60
  out=$(st_run)
  st_check "the good sample after it says nothing" "0" "$(echo "$out" | grep -c 'suppressed by COMPONENT_CHECK_MAIL')"

  echo
  echo "3. two consecutive bad samples still alert"
  rm -f "$st_d/state" "$st_d/pending"
  st_payload degraded 100000
  out=$(st_run)
  st_check "the first is still silent" "0" "$(echo "$out" | grep -c 'suppressed by COMPONENT_CHECK_MAIL')"
  out=$(st_run)
  st_check "the second alerts" "1" "$(echo "$out" | grep -c 'suppressed by COMPONENT_CHECK_MAIL')"
  st_check "  naming the component in the subject" "1" \
    "$(echo "$out" | grep -c 'suppressed by COMPONENT_CHECK_MAIL.*nasa_apod')"

  echo
  echo "4. and it recovers"
  st_payload operational 60
  out=$(st_run)
  st_check "recovery is mailed once the problem clears" "1" "$(echo "$out" | grep -c 'RECOVERED')"

  echo
  if [ "$st_fail" = "0" ]; then echo "selftest passed"; else echo "SELFTEST FAILED"; fi
  exit "$st_fail"
fi

# ── Ask the backend ───────────────────────────────────────────────────────────

body=$(curl -s --max-time 15 -w '\n%{http_code}' "$URL" 2>/dev/null)
curl_rc=$?
http=$(printf '%s' "$body" | tail -n 1)
payload=$(printf '%s' "$body" | sed '$d')

if [ "$curl_rc" -ne 0 ] || [ "$http" != "200" ]; then
  if [ "$ACCEPT" = "1" ]; then
    echo "cannot accept a baseline: endpoint unreachable (curl=$curl_rc http=${http:-none})" >&2
    exit 1
  fi
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
  if [ "$ACCEPT" = "1" ]; then
    echo "cannot accept a baseline: /api/health did not parse (${parsed:-empty})" >&2
    exit 1
  fi
  echo "$(date -u): /api/health did not parse (${parsed:-empty}). Not alerting; healthcheck.sh owns the endpoint."
  exit 0
fi

overall=$(printf '%s\n' "$parsed" | head -n 1 | awk '{print $2}')
now=$(date -u +%s)

# ── Is the list itself still the list ─────────────────────────────────────────
#
# Everything below this reads the components the payload happened to carry and
# asks of each whether it is fresh. That question cannot be asked of a component
# that is no longer there. The question before it is whether the set is the set.
# shellcheck source=component-baseline.sh
. "$(dirname "$0")/component-baseline.sh"

present=$(printf '%s\n' "$parsed" | tail -n +2 | awk 'NF {print $1}' | sort -u | tr '\n' ' ' | sed 's/ *$//')
baseline_compare "$BASELINE" "$present"

if [ "$ACCEPT" = "1" ]; then
  case "$BASELINE_STATE" in
    empty)
      echo "cannot accept a baseline: /api/health parsed but published no components" >&2
      exit 1 ;;
    unset)
      echo "no baseline yet, recording the current set:" ;;
    same)
      echo "the baseline already matches what is published:" ;;
    changed)
      echo "updating the baseline:"
      [ -n "$BASELINE_GONE" ] && echo "  no longer published: $BASELINE_GONE"
      [ -n "$BASELINE_NEW" ]  && echo "  newly published:     $BASELINE_NEW" ;;
  esac
  echo "  $present"
  baseline_accept "$BASELINE" "$present" || exit 1
  echo "written to $BASELINE"
  echo "Astraeusio component baseline accepted: $present" | logger -t astraeusio-components -p daemon.notice
  exit 0
fi

# Problems with the list rather than with a component in it. Kept apart from
# `bad` because they are not degradation and must not be reported as it, and
# because their alert key needs a prefix: `celestrak` degraded and `celestrak`
# absent are different problems and the escalation clock should not carry over
# from one to the other.
structural=()
key_extra=()

case "$BASELINE_STATE" in
  empty)
    structural+=("/api/health published no components at all")
    key_extra+=("components:empty") ;;
  unset)
    structural+=("no component baseline recorded, so a component going away cannot be seen. Run: /opt/astraeusio/component-check.sh --accept-components")
    key_extra+=("baseline:unset") ;;
  changed)
    for gone in $BASELINE_GONE; do
      structural+=("$gone is in the baseline and /api/health no longer publishes it")
      key_extra+=("absent:$gone")
    done ;;
esac

# A component joining is not a fault and does not mail. It is said here and in
# the journal so a deploy that adds one leaves a trace, and because it is worth
# knowing that a new component is not protected until the baseline is accepted.
if [ -n "${BASELINE_NEW:-}" ]; then
  echo "$(date -u): newly published, not in the baseline: $BASELINE_NEW"
  echo "Astraeusio components newly published: $BASELINE_NEW. Accept with component-check.sh --accept-components" \
    | logger -t astraeusio-components -p daemon.notice
fi

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
current=$( { printf '%s\n' "${bad[@]:-}" | awk 'NF {print $1}'
             printf '%s\n' "${key_extra[@]:-}"
           } | awk 'NF' | sort -u | tr '\n' ' ' | sed 's/ *$//')

# Whether this is new, an escalation on age, or something already said. Shared
# with poller-check.sh and backup-check.sh, so the rule lives in one file with a
# self test rather than in three copies that drift.
# shellcheck source=alert-state.sh
. "$(dirname "$0")/alert-state.sh"

# Debounce before deciding. A name has to survive CONFIRM_RUNS consecutive runs
# to reach the alert machinery at all, so a single bad sample produces neither
# an alert now nor a recovery mail on the next run.
mkdir -p "$(dirname "$PENDING")"
alert_confirm "$PENDING" "$CONFIRM_RUNS" $current
current=$ALERT_CONFIRMED

alert_decide "$STATE" "$current"

# Keyed on the confirmed set, not the raw one. A run whose only problems are
# still unconfirmed has nothing to say, and saying nothing is the whole point.
if [ -z "$current" ]; then
  if [ -n "$ALERT_PENDING" ]; then
    echo "$(date -u): seen but not yet confirmed, no alert: $ALERT_PENDING"
  fi
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

if [ "${#bad[@]}" -eq 0 ]; then
  headline="component list changed"
elif [ "${#structural[@]}" -eq 0 ]; then
  headline="components degraded"
else
  headline="components degraded and the list changed"
fi

report="Astraeusio component check FAILED at $(date -u)

$(printf '  %s\n' ${bad[@]+"${bad[@]}"} ${structural[@]+"${structural[@]}"})

overall: $overall
source:  $URL
host:    $(hostname)

A component is degraded when its newest observation is older than the limit the
backend keeps for that series. It means the poller is no longer writing, whether
or not anything logged an error.

A component named as absent is a different thing: the backend has stopped
publishing it, which is always a deploy and never a fault in the data. If the
removal was intended, accept it and this clears:

  /opt/astraeusio/component-check.sh --accept-components"

echo "$(date -u): component check FAILED (overall=$overall)"
printf '  %s\n' ${bad[@]+"${bad[@]}"} ${structural[@]+"${structural[@]}"}
echo "$report" | logger -t astraeusio-components -p daemon.err

# Once per distinct set of bad components, so one that stays degraded does not
# mail every run, and again on age at six hours, at a day, and daily after that.
# The seventeen hour outage on 2026-08-31 sent exactly one mail before this.
case "$ALERT_ACTION" in
  new)
    send_mail "[ALERT] Astraeusio $headline: $current" "$report"
    ;;
  escalate)
    send_mail "[STILL FAILING $ALERT_AGE_H] Astraeusio components: $current" \
      "$report

Degraded for $ALERT_AGE_H and still failing."
    ;;
  *)
    echo "  (already alerted for: $current, degraded for $ALERT_AGE_H)"
    ;;
esac

exit "$EXIT_ALERT_SENT"
