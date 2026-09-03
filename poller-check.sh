#!/bin/bash
# Surfaces a data source whose poller keeps failing.
#
# Every source polls on a timer and logs at ERROR when a fetch fails. Nothing
# watched those lines, so when NOAA retired the magnetometer feed the backend
# logged the same 404 every sixty seconds for forty days, the imf table stopped
# growing, and every health surface stayed green because one Kp query stood in
# for the whole of NOAA. This is the cheapest place to catch that.
#
# Fires when one source errors repeatedly inside the window, and separately
# when a source errors in several consecutive windows, which catches the slow
# pollers that cannot reach the burst count in one hour. Clears on its own once
# a source stops erroring.
#
# Alerting matches backup-check.sh: journald, a state file, and mail only if
# the host has a mailer. Exit status is 0 when healthy and 1 when an alert
# fired, so it can be run by hand and read by a human.
#
# It mails on the transition only, keyed on which sources are alerting rather
# than on the message, and defers to component-check.sh for a source whose
# component is already reporting degraded. `--selftest` checks the mapping that
# deference depends on.
set -uo pipefail

# Exit 10 means the check found a problem and has already notified. cron-run.sh
# treats it as reported and stays quiet, so one event produces one mail. Any
# other non-zero means the check itself broke, which cron-run.sh does mail,
# because at that point nothing else will.
EXIT_ALERT_SENT=10


CONTAINER=${POLLER_CHECK_CONTAINER:-astraeusio-backend-1}
STATE=${POLLER_CHECK_STATE:-/var/lib/astraeusio-poller-state}
# Separate from STATE, which is per source streak bookkeeping. This one holds a
# single line: which sources the last mail was about.
ALERT_STATE=${POLLER_CHECK_ALERT_STATE:-/var/lib/astraeusio-poller-alerted}
WINDOW=${POLLER_CHECK_WINDOW:-60m}
BURST=5      # errors from one source inside a single window
STREAK=3     # consecutive windows in which a source errored at all

# Retried successes: the poll worked, but only on a later attempt. Deliberately
# a higher bar than the ERROR rules above, because one ridden-through blip is
# exactly what the retry is for and must stay quiet.
#
# The bar is a proportion rather than a count, because a flat number cannot suit
# both ends: the ISS polls 720 times an hour and APOD once, so five retries is
# noise for one and impossible for the other. A source must need a retry on at
# least a quarter of its polls before this says anything, which healthy sources
# never approach, and never on fewer than RETRY_MIN retries so a low rate source
# cannot trip on a single event. Low rate sources are covered by the ERROR rules
# instead, since for them an exhausted retry is the normal way a fault shows.
RETRY_MIN=5      # never alert on fewer than this many retried successes
RETRY_PCT=25     # ...and not unless they are this share of the source's polls
RETRY_STREAK=3   # or this many consecutive windows at RETRY_MIN, for a slow burn

# Throughput: is the source delivering at the rate its interval implies.
#
# The gap this closes: a poller that slows without failing looks perfectly
# healthy on every other signal here, because every other rule counts errors.
# On 2026-08-18 the ISS upstream degraded from a 0.055s median to about 1.15s
# and hour 12 delivered 585 of an expected 720 samples, a 19% loss, with zero
# ERROR lines. Nothing alerted, because nothing was measuring whether anything
# arrived. That is the same blindness as the frozen IMF table wearing a
# different hat.
#
# Two conditions, both required, so this cannot fire on ordinary drift. The
# ratio catches a real collapse; the absolute floor stops a low count source
# tripping on granularity. xray expects only 30 polls an hour, so three missed
# polls is already 10%, and its healthy minimum over 228 measured hours is
# exactly 90.0%. A ratio alone would sit one missed poll away from mailing.
#
# Measured healthy minimums over 228 hours per source, for the record:
#   imf 96.7%   kp 96.7%   solar-wind 96.7%   xray 90.0%   iss 97.5% at p5
# Simulated over all 1140 source-hours these thresholds fire 7 times, all of
# them the two real ISS incidents, and never otherwise.
THROUGHPUT_SOFT=90    # below this share of the expected rate is a concern
THROUGHPUT_HARD=75    # below this is an outage and alerts on the first window
THROUGHPUT_STREAK=3   # consecutive soft windows before it says anything
THROUGHPUT_MIN_MISSED=10  # never alert on a smaller absolute shortfall
THROUGHPUT_MIN_EXPECTED=30  # and only where the rate is high enough to mean anything
# Delivery moved to notify.sh, which owns the recipient, the sender and
# the API key. Leaving these here would read as if they still controlled
# where alerts go, and they do not.

# Exercisable without alerting. NOTIFY_SEND=0 silences every script on this
# host at once, since all of them send through notify.sh; this is the local
# switch, for exercising this check alone. Both exist because neither was set
# when a harness ran poller-check.sh against fixture logs and mailed 37 alerts
# in eleven seconds. The harness isolated state and data correctly and did not
# isolate the effect the script exists to produce.
SEND_MAIL=${POLLER_CHECK_MAIL:-1}

send_mail() {
  local subject=$1 body=$2
  if [ "$SEND_MAIL" != "1" ]; then
    echo "  (mail suppressed by POLLER_CHECK_MAIL=$SEND_MAIL: $subject)"
    return 0
  fi
  /opt/astraeusio/notify.sh "$subject" "$body" 2>&1 | sed 's/^/  /'
}

# ── Deference to component-check.sh ───────────────────────────────────────────
#
# Both checks see one outage when a slow poller dies: this one sees the ERROR
# lines, component-check.sh sees the series cross its freshness limit. The APOD
# outage on 2026-08-13 sent four mails from the two of them for one event.
#
# Whoever detects it first and most specifically owns the telling. When a
# source's component is already reporting degraded, component-check.sh has it
# and has already mailed, so this one logs the deferral and stays quiet.
#
# Everything else alerts. That includes a poller with no mapping and a health
# endpoint that cannot be reached or does not parse, because deferring to
# something that might itself be down is how a blind spot gets built, and a
# blind spot is what this check exists to prevent.
# Reaches the origin through the internal listener on 8081, which is plaintext
# and asks for no client certificate. Until 2026-08-14 this went to 443 with
# --resolve, which succeeds only while ssl_verify_client is `optional`; the
# dependency was invisible and shared with three other callers. See the 8081
# server block in frontend/nginx.conf.
HEALTH_URL=${POLLER_CHECK_HEALTH_URL:-http://127.0.0.1:8081/api/health}
# The components the backend is expected to publish, written only by
# `component-check.sh --accept-components`. Read here to decide whose alarm a
# missing component is, never to alert on the list itself: component-check.sh
# owns that, and two mails for one removal is the noise this file spent three
# revisions removing.
BASELINE=${POLLER_CHECK_BASELINE:-/var/lib/astraeusio-components-baseline}
# shellcheck source=component-baseline.sh
. "$(dirname "$0")/component-baseline.sh"

# Written out, not derived. poller/apod feeds nasa_apod, poller/starlink feeds
# celestrak, poller/forecast feeds ml_forecast: no rule turns one name into the
# other, and a rule that appeared to would break silently the first time it was
# wrong. An empty value means no component tracks that poller, which is a
# recorded decision rather than an omission. A poller missing from this table
# alerts rather than defers, and --selftest fails on it.
declare -A COMPONENT_OF=(
  [poller/kp]=noaa_kp
  [poller/kp-3h]=noaa_kp_3h
  [poller/solar-wind]=noaa_solar_wind
  [poller/xray]=noaa_xray
  [poller/imf]=noaa_imf
  [poller/dst]=noaa_dst
  [poller/iss]=iss
  [poller/apod]=nasa_apod
  [poller/neo]=nasa_neo
  [poller/epic]=nasa_epic
  [poller/exoplanets]=nasa_exoplanets
  [poller/starlink]=celestrak
  [poller/forecast]=ml_forecast
  # alerts is not a freshness reading. The feed is episodic, so no row age
  # separates a quiet sun from a dead feed, and its component reports the
  # verdict the poller records each cycle instead: did the fetch work, did it
  # come back with anything, is the window it returned still being added to.
  # Mapped 2026-08-31, when the backend started publishing that component.
  [poller/alerts]=noaa_alerts
  # No component reports these, so they alert here or nowhere.
  [poller/health]=""
  # Retention deletes rows past each table's window once a day. It feeds no
  # component and logs one line per run, so there is nothing to defer to.
  # Added 2026-09-01 with the poller.
  [poller/retention]=""
  # anomaly is the local detector, not a feed. It reads what the other pollers
  # wrote and logs only when it finds something, so it has no per-run line and
  # no upstream. It was absent from this table until 2026-08-22 and the
  # selftest could not see the hole, because the selftest enumerates pollers
  # from the log and this one never writes to it.
  [poller/anomaly]=""
)

health_states=""
health_loaded=0

# Fills health_states with "component status" lines. Non-zero on any failure to
# reach or parse, which the caller turns into "alert on everything".
load_health() {
  local raw http payload
  raw=$(curl -s --max-time 15 -w '\n%{http_code}' "$HEALTH_URL" 2>/dev/null) || return 1
  http=$(printf '%s' "$raw" | tail -n 1)
  [ "$http" = "200" ] || return 1
  payload=$(printf '%s' "$raw" | sed '$d')
  health_states=$(printf '%s' "$payload" | python3 -c '
import sys, json
d = json.load(sys.stdin)
for n, c in sorted(d["components"].items()):
    print(n, c.get("status", "?") if isinstance(c, dict) else str(c))
' 2>/dev/null) || return 1
  [ -n "$health_states" ]
}

# Whether a component this table names is still published, and if not, whose
# problem that is. Echoes one of:
#
#   published  the payload carries it, nothing to say
#   deferred   it has left a baseline the operator has not accepted, so
#              component-check.sh is already mailing about the list
#   fault      it is in neither, so this table names something that does not
#              exist and the deference built on it is dead
#
# A function rather than three lines inline, because a rule that only exists
# inside the loop that applies it can only be tested by running the loop.
mapping_verdict() {  # component, published set, baseline-gone set
  case " $2 " in *" $1 "*) echo published; return ;; esac
  case " $3 " in *" $1 "*) echo deferred; return ;; esac
  echo fault
}

# Returns 0 only on a clear "component-check.sh already owns this". Every other
# path returns 1, so the default is always to alert.
already_reported() {
  local name=$1 comp st
  [ "$health_loaded" = "1" ] || return 1
  if [ -z "${COMPONENT_OF[$name]+isset}" ]; then
    unmapped+=("$name")
    return 1
  fi
  comp=${COMPONENT_OF[$name]}
  [ -n "$comp" ] || return 1
  st=$(printf '%s\n' "$health_states" | awk -v c="$comp" '$1 == c { print $2 }')
  [ -n "$st" ] || return 1
  [ "$st" = "operational" ] && return 1
  return 0
}

# Expected poll rates, read from the backend's own boot line rather than kept
# here. A second copy of the interval table in bash would drift from
# PollConfig::from_env the first time anything changed, and be wrong in both
# directions; the same reasoning kept component-check.sh from copying
# SERIES_FRESHNESS. The boot line is far outside the 60m window, so it is
# cached and re-read only when the container has restarted.
INTERVAL_CACHE=${POLLER_CHECK_INTERVAL_CACHE:-/var/lib/astraeusio-poller-intervals}

load_intervals() {
  local started cached
  started=$(docker inspect -f '{{.State.StartedAt}}' "$CONTAINER" 2>/dev/null || echo unknown)
  cached=$(head -n 1 "$INTERVAL_CACHE" 2>/dev/null || true)
  if [ "$cached" != "$started" ] || [ ! -s "$INTERVAL_CACHE" ]; then
    local boot
    # Unbounded on purpose: the line is written once at boot. Not piped into
    # grep -q, because that kills the producer and pipefail reads the SIGPIPE
    # as failure, which is how a deploy check once reported a healthy site dead.
    boot=$(docker logs "$CONTAINER" 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep 'intervals loaded' | tail -1 || true)
    [ -z "$boot" ] && return 1
    {
      echo "$started"
      printf '%s\n' "$boot" | grep -oE '[a-z0-9_]+=[0-9]+' \
        | grep -v '^retry_count=' | tr '_' '-' | tr '=' ' '
    } > "$INTERVAL_CACHE.new"
    mv "$INTERVAL_CACHE.new" "$INTERVAL_CACHE"
  fi
  return 0
}

expected_per_window() {  # $1 = bare source name, echoes expected polls or nothing
  local iv
  iv=$(awk -v n="$1" 'NR > 1 && $1 == n { print $2; exit }' "$INTERVAL_CACHE" 2>/dev/null)
  [ -z "${iv:-}" ] && return 1
  [ "$iv" -le 0 ] && return 1
  echo $(( WINDOW_SECS / iv ))
}

# WINDOW is a duration like 60m. Only what the throughput rule needs.
case "$WINDOW" in
  *m) WINDOW_SECS=$(( ${WINDOW%m} * 60 )) ;;
  *h) WINDOW_SECS=$(( ${WINDOW%h} * 3600 )) ;;
  *s) WINDOW_SECS=${WINDOW%s} ;;
  *)  WINDOW_SECS=3600 ;;
esac

problems=()
recovered=()
# Problems with no source attached, which can never be deferred.
global_problems=()
# Messages held per source until deference has had its say.
declare -A msgs
alerting_names=()
deferred=()
unmapped=()

add_problem() {
  msgs[$1]="${msgs[$1]:-}$2"$'\n'
}

# Overridable so the thresholds can be exercised against a fixture instead of
# waiting for a real upstream to flap.
LOG_CMD=${POLLER_CHECK_LOG_CMD:-docker logs --since $WINDOW $CONTAINER}
if ! logs=$($LOG_CMD 2>&1); then
  global_problems+=("cannot read logs for $CONTAINER")
  logs=""
fi

# Strip the colour codes tracing writes, keep the ERROR lines, and pull the
# source field out of each one. Only poller sources are counted here.
counts=$(printf '%s\n' "$logs" \
  | sed 's/\x1b\[[0-9;]*m//g' \
  | grep 'ERROR' \
  | grep -o 'source="poller/[^"]*"' \
  | sed 's/source="//; s/"$//' \
  | sort | uniq -c | awk '{print $2, $1}')

# A retried success names itself, so it can be counted the same way as an error.
clean=$(printf '%s\n' "$logs" | sed 's/\x1b\[[0-9;]*m//g')

retry_counts=$(printf '%s\n' "$clean" \
  | grep 'succeeded after retry' \
  | grep -o 'source="poller/[^"]*"' \
  | sed 's/source="//; s/"$//' \
  | sort | uniq -c | awk '{print $2, $1}')

declare -A retried
while read -r name count; do
  [ -z "${name:-}" ] && continue
  retried[$name]=$count
done <<< "$retry_counts"

# Denominator: every line mentioning that source, which covers the INFO success
# lines as well as the WARN and ERROR ones, so it approximates polls attempted.
poll_total() {
  # $1 already carries the poller/ prefix. Prepending it again matched
  # nothing, so the denominator was always zero and the percentage rule
  # could never fire, which the fixture for a flapping source caught.
  printf '%s\n' "$clean" | grep -cE "$1[\":]" || true
}

# A pipe into `grep -q` is the one construct banned here, and --selftest fails
# if it reappears. grep -q exits on the first match, the producer takes SIGPIPE
# and exits 141, and `set -o pipefail` reports the match as a failure. It is
# latent rather than always wrong: it only bites once the producer's output
# exceeds the 64KB pipe buffer, measured at 7.6KB correct and 15.9MB broken, so
# reading the code cannot tell you which you have. It broke deploy.sh in August
# 2026 and then poller-check.sh eight lines below a comment warning about it,
# which is why this is a test and not a comment. Use `grep -q pat <<< "$var"`,
# or `case`, or grep a file.

# ── Selftest ──────────────────────────────────────────────────────────────────
#
# The deference rule is only as safe as its mapping. A poller added later with
# no entry here would defer to nothing and, if the check ever stopped defaulting
# to alert, go silent. This fails loudly instead.
if [ "${1:-}" = "--selftest" ]; then
  st_fail=0
  st_check() {  # description, expected, actual
    if [ "$2" = "$3" ]; then
      echo "  ok    $1"
    else
      echo "  FAIL  $1 (expected '$2', got '$3')"
      st_fail=1
    fi
  }

  echo "poller-check.sh --selftest"
  echo
  echo "1. every poller the backend emits has an explicit mapping"
  st_log_cmd=${POLLER_CHECK_SELFTEST_LOG_CMD:-docker logs $CONTAINER}
  # Two sources, unioned. The log misses a poller that never writes to it, which
  # is how poller/anomaly sat unmapped and unnoticed; the interval table the
  # backend prints at boot names every poller whether or not it ever logs.
  st_pollers=$( { $st_log_cmd 2>&1 | sed 's/\x1b\[[0-9;]*m//g' \
      | grep -oE 'poller/[a-z0-9-]+'
    load_intervals >/dev/null 2>&1 \
      && tail -n +2 "$INTERVAL_CACHE" 2>/dev/null | awk 'NF {print "poller/" $1}'
  } | sort -u)
  if [ -z "$st_pollers" ]; then
    echo "  FAIL  no pollers found in the log, cannot check the mapping"
    st_fail=1
  fi
  while read -r p; do
    [ -z "${p:-}" ] && continue
    if [ -z "${COMPONENT_OF[$p]+isset}" ]; then
      echo "  FAIL  $p has no entry in COMPONENT_OF"
      st_fail=1
    else
      echo "  ok    $p -> ${COMPONENT_OF[$p]:-(no component, alerts here or nowhere)}"
    fi
  done <<< "$st_pollers"

  echo
  echo "2. every mapped component exists in /api/health"
  echo "   (the run does this too now, against the baseline; this is the by-hand"
  echo "    version that names every mapping rather than only the broken ones)"
  if load_health; then
    health_loaded=1
    for p in "${!COMPONENT_OF[@]}"; do
      c=${COMPONENT_OF[$p]}
      [ -z "$c" ] && continue
      if printf '%s\n' "$health_states" | awk -v c="$c" '$1 == c { found = 1 } END { exit !found }'; then
        echo "  ok    $c is published"
      else
        echo "  FAIL  $p maps to $c, which /api/health does not publish"
        st_fail=1
      fi
    done
  else
    echo "  SKIP  /api/health unreachable, cannot check the component names"
  fi

  echo
  echo "3. an unreachable /api/health alerts rather than defers"
  health_loaded=0; health_states=""
  unmapped=()
  already_reported poller/apod && r=defer || r=alert
  st_check "health not loaded -> alert" alert "$r"

  echo
  echo "4. a health payload that does not parse alerts rather than defers"
  ( HEALTH_URL=http://127.0.0.1:9/nope; load_health ) && r=loaded || r=failed
  st_check "load_health on a dead endpoint" failed "$r"

  echo
  echo "5. with health loaded, each case resolves correctly"
  health_loaded=1
  health_states="nasa_apod degraded
noaa_xray operational
noaa_imf unknown"
  unmapped=()
  already_reported poller/apod   && r=defer || r=alert
  st_check "mapped and degraded -> defer"            defer "$r"
  already_reported poller/xray   && r=defer || r=alert
  st_check "mapped and operational -> alert"         alert "$r"
  already_reported poller/imf    && r=defer || r=alert
  st_check "mapped and unknown -> defer"             defer "$r"
  already_reported poller/neo    && r=defer || r=alert
  st_check "mapped but absent from payload -> alert" alert "$r"
  already_reported poller/health && r=defer || r=alert
  st_check "mapped to no component -> alert"         alert "$r"
  already_reported poller/nosuch && r=defer || r=alert
  st_check "not in the table at all -> alert"        alert "$r"
  st_check "and it was recorded as unmapped"         "poller/nosuch" "${unmapped[0]:-}"

  echo
  echo "6. a mapping the backend no longer publishes is classified, not ignored"
  st_check "still published -> nothing to say"                   published "$(mapping_verdict noaa_kp "noaa_kp celestrak" "")"
  st_check "gone from a baseline not yet accepted -> deferred"   deferred  "$(mapping_verdict celestrak "noaa_kp" "celestrak")"
  st_check "gone and not in the baseline either -> this table"   fault     "$(mapping_verdict nasa_imf "noaa_kp" "celestrak")"
  st_check "no baseline at all -> this table, never silence"     fault     "$(mapping_verdict celestrak "noaa_kp" "")"
  # A substring must not read as a member: noaa_kp is not noaa_kp_3h.
  st_check "names match whole, not by prefix"                    fault     "$(mapping_verdict noaa_kp "noaa_kp_3h" "")"

  echo
  echo "7. no script pipes into grep -q, the banned construct"
  st_pipe_hits=$(grep -ln '|[[:space:]]*grep[^|]*-[a-zA-Z]*q' /opt/astraeusio/*.sh 2>/dev/null \
    | grep -v 'poller-check.sh' || true)
  if [ -n "$st_pipe_hits" ]; then
    echo "  FAIL  a pipe into grep -q was found in:"
    printf '%s\n' "$st_pipe_hits" | sed 's/^/          /'
    st_fail=1
  else
    st_script_count=$(find /opt/astraeusio -maxdepth 1 -name "*.sh" | wc -l)
    echo "  ok    none in $st_script_count scripts"
  fi
  # This file is excluded from the scan above because the comment describing the
  # construct contains it. Checked separately, against real code lines only.
  st_self=$(grep -n '|[[:space:]]*grep[^|]*-[a-zA-Z]*q' /opt/astraeusio/poller-check.sh 2>/dev/null \
    | grep -v '^[0-9]*:[[:space:]]*#' || true)
  if [ -n "$st_self" ]; then
    echo "  FAIL  poller-check.sh itself has one, outside a comment:"
    printf '%s\n' "$st_self" | sed 's/^/          /'
    st_fail=1
  else
    echo "  ok    poller-check.sh has it only inside comments"
  fi

  echo
  if [ "$st_fail" = "0" ]; then echo "selftest passed"; else echo "SELFTEST FAILED"; fi
  exit "$st_fail"
fi

# Delivered polls per source: every line naming it that is not an ERROR, less
# the extra line a retried success writes, since that logs a WARN and an INFO
# for one poll and would otherwise inflate the count.
delivered_count() {  # $1 carries the poller/ prefix
  local total errs retries
  total=$(printf '%s\n' "$clean" | grep -cE "$1[\":]" || true)
  errs=$(printf '%s\n' "$clean" | grep -E "$1[\":]" | grep -c 'ERROR' || true)
  retries=$(printf '%s\n' "$clean" | grep -E "$1[\":]" | grep -c 'succeeded after retry' || true)
  echo $(( total - errs - retries ))
}

# A window containing a restart is a partial window by definition and its counts
# mean nothing. Skipping it is cheaper than trying to prorate it.
#
# Matched with `case`, not `printf | grep -q`. grep -q exits on the first hit
# and kills the producer with SIGPIPE, which pipefail then reports as failure,
# so the match reads as no-match and the window is scored instead of skipped.
# deploy.sh lost a day to the same construct in August 2026, and the comment
# warning about it is eight lines above this one, which did not stop me writing
# it again. The fixture for a restart inside the window is what caught it.
window_had_restart=0
case "$clean" in *"intervals loaded"*) window_had_restart=1 ;; esac

have_intervals=0
load_intervals && have_intervals=1

# Previous state, one line per source: name streak alerting
declare -A prev_streak prev_alerting prev_rstreak prev_tstreak
if [ -f "$STATE" ]; then
  while read -r name streak alerting rstreak tstreak; do
    [ -z "${name:-}" ] && continue
    prev_streak[$name]=$streak
    prev_alerting[$name]=$alerting
    prev_rstreak[$name]=${rstreak:-0}
    prev_tstreak[$name]=${tstreak:-0}
  done < "$STATE"
fi

declare -A now_count
while read -r name count; do
  [ -z "${name:-}" ] && continue
  now_count[$name]=$count
done <<< "$counts"

# Every source seen now or previously, so a source that stops erroring is still
# visited and can clear.
declare -A seen
for name in "${!now_count[@]}"; do seen[$name]=1; done
for name in "${!prev_streak[@]}";  do seen[$name]=1; done
for name in "${!retried[@]}";      do seen[$name]=1; done
# A source delivering nothing logs nothing, so it would never enter `seen` by
# any of the three sets above and its collapse would be invisible to the very
# rule written to catch it. The interval table names every poller that exists.
if [ "$have_intervals" = "1" ]; then
  while read -r iname _; do
    [ -z "${iname:-}" ] && continue
    seen[poller/$iname]=1
  done < <(tail -n +2 "$INTERVAL_CACHE" 2>/dev/null)
fi

mkdir -p "$(dirname "$STATE")"
: > "$STATE.new"

for name in "${!seen[@]}"; do
  count=${now_count[$name]:-0}
  streak=${prev_streak[$name]:-0}
  was_alerting=${prev_alerting[$name]:-0}

  # A window counts toward a streak only when the source got nothing through in
  # it. The old test was "did it error at all", which scores the same for a
  # source polling once an hour and one polling thirty times: apod failing its
  # only poll lost the hour, xray failing one poll of thirty lost nothing, and
  # both counted 1. Six of the eight alerts this check had raised came from that
  # conflation, every one of them on a source still delivering data.
  #
  # A successful poll is any other line the source logged: the count line, a
  # no-change, an empty payload, a partial parse, a retried success. A poll that
  # fails logs one ERROR and nothing else, so "nothing got through" is exactly
  # successes of zero, and the burst rule below still owns the fast collapse.
  #
  # What this deliberately stops catching: a source that fails some polls every
  # window forever, below the burst count. It loses no data at the poller, and
  # if it ever does go stale it crosses the freshness limit and
  # component-check.sh picks it up by reading data rather than logs.
  total=$(poll_total "$name")
  successes=$(( total - count ))
  if [ "$count" -gt 0 ] && [ "$successes" -le 0 ]; then
    streak=$((streak + 1))
  else
    streak=0
  fi

  alerting=0
  if [ "$count" -ge "$BURST" ]; then
    alerting=1
    add_problem "$name" "$name failed $count times in the last $WINDOW"
  elif [ "$streak" -ge "$STREAK" ]; then
    alerting=1
    add_problem "$name" "$name got nothing through in $streak consecutive windows, $count failed polls in the last $WINDOW"
  fi

  # Retried successes for this source, evaluated on their own bar.
  r=${retried[$name]:-0}
  rstreak=${prev_rstreak[$name]:-0}
  if [ "$r" -ge "$RETRY_MIN" ]; then
    rstreak=$(( rstreak + 1 ))
    total=$(poll_total "$name")
    if [ "${total:-0}" -gt 0 ]; then
      pct=$(( r * 100 / total ))
      if [ "$pct" -ge "$RETRY_PCT" ]; then
        alerting=1
        add_problem "$name" "$name needed a retry on $r of $total polls (${pct}%) in the last $WINDOW, the upstream is flapping"
      fi
    fi
  else
    rstreak=0
  fi
  # A slow burn that never reaches the percentage in any single window but never
  # stops either. Without this, a steady one in ten failure rate stays silent
  # forever, which is the blindness the retry would otherwise have introduced.
  if [ "$rstreak" -ge "$RETRY_STREAK" ]; then
    alerting=1
    add_problem "$name" "$name has needed retries in $rstreak consecutive windows, $r in the last $WINDOW"
  fi

  # Throughput. Deliberately last, because it is the only rule that can fire
  # while the source is reporting no errors at all.
  tstreak=${prev_tstreak[$name]:-0}
  bare=${name#poller/}
  expected=""
  # Only a source that feeds a component logs one line per successful poll, and
  # only for those does a delivery rate mean anything. anomaly runs every 60s
  # and logs nothing unless it detects something, so measuring it against 60
  # expected polls an hour read as a total outage and would have mailed on the
  # first run. An empty or absent component is the marker for "not a feed".
  tracks_a_series=0
  [ -n "${COMPONENT_OF[$name]:-}" ] && tracks_a_series=1
  if [ "$have_intervals" = "1" ] && [ "$window_had_restart" = "0" ] \
     && [ "$tracks_a_series" = "1" ]; then
    expected=$(expected_per_window "$bare" || true)
  fi
  if [ -n "${expected:-}" ] && [ "$expected" -ge "$THROUGHPUT_MIN_EXPECTED" ]; then
    got=$(delivered_count "$name")
    [ "$got" -lt 0 ] && got=0
    missed=$(( expected - got ))
    pct=$(( got * 100 / expected ))
    if [ "$missed" -ge "$THROUGHPUT_MIN_MISSED" ] && [ "$pct" -lt "$THROUGHPUT_HARD" ]; then
      alerting=1
      tstreak=$(( tstreak + 1 ))
      add_problem "$name" "$name delivered $got of an expected $expected polls (${pct}%) in the last $WINDOW, the upstream is slow rather than failing"
    elif [ "$missed" -ge "$THROUGHPUT_MIN_MISSED" ] && [ "$pct" -lt "$THROUGHPUT_SOFT" ]; then
      tstreak=$(( tstreak + 1 ))
      if [ "$tstreak" -ge "$THROUGHPUT_STREAK" ]; then
        alerting=1
        add_problem "$name" "$name has delivered under ${THROUGHPUT_SOFT}% of its expected rate for $tstreak consecutive windows, $got of $expected (${pct}%) in the last $WINDOW"
      fi
    else
      tstreak=0
    fi
  else
    tstreak=0
  fi

  if [ "$was_alerting" = "1" ] && [ "$alerting" = "0" ] && [ "$count" -eq 0 ]; then
    recovered+=("$name is no longer failing")
  fi

  # Drop a source that is quiet and was not alerting, so the file does not grow
  # a permanent entry for every source that ever hiccupped.
  if [ "$streak" -gt 0 ] || [ "$alerting" = "1" ] || [ "$was_alerting" = "1" ] \
     || [ "$rstreak" -gt 0 ] || [ "$tstreak" -gt 0 ]; then
    echo "$name $streak $alerting $rstreak $tstreak" >> "$STATE.new"
  fi
done

mv "$STATE.new" "$STATE"

for line in "${recovered[@]:-}"; do
  [ -z "$line" ] && continue
  echo "$(date -u): $line"
  echo "Astraeusio poller recovered at $(date -u): $line" | logger -t astraeusio-poller -p daemon.notice
done

# ── Which of these are ours to tell ───────────────────────────────────────────
[ "${#global_problems[@]}" -gt 0 ] && problems+=("${global_problems[@]}") && alerting_names+=("container-logs")

# Loaded on every run now, not only when there is something to defer. The
# mapping check below needs it, and a table that has gone stale is worth
# knowing about on a quiet hour as much as on a loud one.
if load_health; then
  health_loaded=1
else
  echo "$(date -u): /api/health unreachable or unparseable, alerting on everything rather than deferring"
fi

# ── Does this table still describe the backend ────────────────────────────────
#
# Every component named here must still be published, and nothing on the cron
# path checked that: `already_reported` looks a name up among the components the
# payload carried, so a name that has left simply falls through to alerting.
# Nothing under-reports, which is why this went unnoticed, but the deference is
# dead and stays dead. --selftest caught it and --selftest runs when someone
# remembers to run it.
#
# Whose alarm it is depends on the baseline. A name that has left a set the
# operator has not yet accepted is component-check.sh's to mail and this only
# says so. A name in neither the payload nor the baseline is this table's own
# fault: either it was never right, or the removal was accepted and this was
# not updated with it.
if [ "$health_loaded" = "1" ]; then
  published=$(printf '%s\n' "$health_states" | awk 'NF {print $1}' | sort -u | tr '\n' ' ' | sed 's/ *$//')
  baseline_compare "$BASELINE" "$published"
  for p in $(printf '%s\n' "${!COMPONENT_OF[@]}" | sort); do
    c=${COMPONENT_OF[$p]}
    [ -z "$c" ] && continue
    case "$(mapping_verdict "$c" "$published" "$BASELINE_GONE")" in
      published) ;;
      deferred)
        deferred+=("$p, because $c has left /api/health and component-check.sh owns the list") ;;
      fault)
        problems+=("$p maps to $c, which /api/health does not publish and the baseline does not have")
        alerting_names+=("mapping/$p") ;;
    esac
  done
fi

# Guarded. An empty associative array is unset as far as ${!msgs[@]} and
# ${#msgs[@]} are concerned under set -u, unlike an empty indexed array, so
# both of those print an unbound variable error on every healthy run. The
# ${msgs[*]+set} form is the one that is safe on both.
if [ -n "${msgs[*]+set}" ]; then
  while read -r name; do
    [ -z "${name:-}" ] && continue
    if already_reported "$name"; then
      deferred+=("$name, because ${COMPONENT_OF[$name]} is already degraded and component-check.sh owns it")
      continue
    fi
    alerting_names+=("$name")
    while IFS= read -r line; do
      [ -n "$line" ] && problems+=("$line")
    done <<< "${msgs[$name]}"
  done <<< "$(printf '%s\n' "${!msgs[@]}" | sort)"
fi

for line in "${deferred[@]:-}"; do
  [ -z "$line" ] && continue
  echo "$(date -u): deferring $line"
  echo "Astraeusio poller deferring $line" | logger -t astraeusio-poller -p daemon.notice
done

# A poller with no mapping alerts, and says so, because silence here would be
# the exact blind spot the deference rule risks creating.
for line in "${unmapped[@]:-}"; do
  [ -z "$line" ] && continue
  echo "$(date -u): $line has no component mapping, alerting rather than deferring"
  echo "Astraeusio poller: $line has no component mapping in poller-check.sh" \
    | logger -t astraeusio-poller -p daemon.warning
done

# Whether this is new, an escalation on age, or something already said. Shared
# with component-check.sh and backup-check.sh so the rule has one home and one
# self test. It runs before the healthy path below, which exits early and needs
# the recovery verdict.
#
# The key is the sorted source names and nothing else. Counts and streak lengths
# change every window by construction, which is why including them once mailed
# "3 consecutive windows" and "4 consecutive windows" for one APOD outage.
current=$(printf '%s\n' ${alerting_names[@]+"${alerting_names[@]}"} | sort -u | tr '\n' ' ' | sed 's/ *$//')
mkdir -p "$(dirname "$ALERT_STATE")"
# shellcheck source=alert-state.sh
. "$(dirname "$0")/alert-state.sh"
alert_decide "$ALERT_STATE" "$current"

if [ "${#problems[@]}" -eq 0 ]; then
  if [ "$ALERT_ACTION" = "recovered" ]; then
    line="Astraeusio poller recovered at $(date -u), after $ALERT_AGE_H. Previously: $ALERT_PREV"
    echo "$(date -u): recovered after $ALERT_AGE_H (was: $ALERT_PREV)"
    echo "$line" | logger -t astraeusio-poller -p daemon.notice
    send_mail "[RECOVERED after $ALERT_AGE_H] Astraeusio poller healthy" "$line" || true
  else
    echo "$(date -u): poller check ok"
  fi
  exit 0
fi

body="Astraeusio poller check failed at $(date -u)

$(printf '  %s\n' "${problems[@]}")

Container: $CONTAINER
Window: $WINDOW
Host: $(hostname)"

echo "$(date -u): poller check FAILED"
printf '  %s\n' "${problems[@]}"

echo "$body" | logger -t astraeusio-poller -p daemon.err

# Mail on the transition only. The key is the sorted set of alerting source
# names, with the counts and the streak lengths deliberately left out: those
# change every window by construction, so a key that included them guaranteed a
# fresh mail every hour for one unchanging problem. APOD mailed "3 consecutive
# windows" at 03:17 and "4 consecutive windows" at 04:17 for the same outage.
# component-check.sh has keyed on the name set since it was written.
#
# A new source joining changes the key and does mail, which is the case worth
# being told about. The counts stay in the body and in journald, where they are
# what you read once you are already looking.


# The subject names the sources. Every alert used to read "Astraeusio poller
# check FAILED", so a mail client threaded them together and thirty-five sends
# were read as four. An identical subject makes repeats collapse, and collapsing
# biases the reader toward under-counting at exactly the moment volume is the
# signal.
short=$(printf '%s\n' "${alerting_names[@]}" | sed 's|^poller/||' | sort -u | paste -sd, - | sed 's/,/, /g')

mkdir -p "$(dirname "$ALERT_STATE")"
case "$ALERT_ACTION" in
  new)
    send_mail "[ALERT] Astraeusio poller: $short" "$body" || true
    ;;
  escalate)
    send_mail "[STILL FAILING $ALERT_AGE_H] Astraeusio poller: $short" \
      "$body

Failing for $ALERT_AGE_H." || true
    ;;
  *)
    echo "  (already alerted for: $current, failing for $ALERT_AGE_H, not mailing again)"
    ;;
esac

exit "$EXIT_ALERT_SENT"
