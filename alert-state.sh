#!/bin/bash
# When to mail about a problem that has not gone away. Sourced, not run.
#
# Every check here mails on the transition and then goes quiet, which was built
# deliberately: poller-check.sh once keyed its mail on the streak counts and sent
# "3 consecutive windows" at 03:17 and "4 consecutive windows" at 04:17 for one
# APOD outage, because a key that changes every run makes every run look new.
#
# The cost of that silence showed on 2026-09-01: noaa_imf and noaa_solar_wind
# alerted continuously from 21:07 the previous evening, seventeen hours, on a
# single mail. A feed dead for twelve hours and one dead for twelve days were
# indistinguishable in the inbox.
#
# So the escalation is on age, not on the data. Thresholds are 6 hours, 24
# hours, and daily after that, and each fires once. That is bounded arithmetic
# rather than a property of the readings: a problem lasting a week sends nine
# mails, not the 672 that mailing every run would send, and not the one it sends
# today.
#
# Six hours for the first step, not one. It leaves room for something to fix
# itself, which the NOAA feed did that same morning, and two mails inside the
# first hour would be the noise this is trying not to recreate.
#
#   alert_decide <state_file> <key>
#
# `key` is the sorted, stable identity of the problem: names, never counts or
# ages. Empty means healthy. Sets:
#
#   ALERT_ACTION  new | escalate | quiet | recovered | ok
#   ALERT_AGE     seconds the current problem set has been unbroken
#   ALERT_AGE_H   that as "17h 20m"
#   ALERT_PREV    on recovery, the key that just cleared
#
# The state file holds "<first_seen> <mailed_threshold> <key>". A file written by
# an older version holds the bare key, which is read as a problem seen now: it
# loses the age of a problem already in progress at upgrade time and cannot
# escalate early on it, which is the safe direction.

# Largest threshold the age has crossed: 0, 6h, then whole days.
_alert_threshold_for() {
  local age=$1
  if   [ "$age" -ge 86400 ]; then echo $(( (age / 86400) * 86400 ))
  elif [ "$age" -ge 21600 ]; then echo 21600
  else echo 0
  fi
}

alert_age_human() {
  local s=$1
  if   [ "$s" -lt 3600 ]; then echo "$(( s / 60 ))m"
  elif [ "$s" -lt 86400 ]; then echo "$(( s / 3600 ))h $(( (s % 3600) / 60 ))m"
  else echo "$(( s / 86400 ))d $(( (s % 86400) / 3600 ))h"
  fi
}

alert_decide() {
  local state=$1 key=$2 now
  now=$(date -u +%s)
  ALERT_ACTION=quiet
  ALERT_AGE=0
  ALERT_AGE_H="0m"
  ALERT_PREV=""

  local first mailed prev raw=""
  [ -f "$state" ] && raw=$(head -n 1 "$state" 2>/dev/null)

  if [ -z "$raw" ]; then
    first=""; mailed=0; prev=""
  elif [[ "${raw%% *}" =~ ^[0-9]+$ ]] && [[ "$raw" == *" "* ]]; then
    first=${raw%% *}
    local rest=${raw#* }
    mailed=${rest%% *}
    prev=${rest#* }
    # A two field line is a timestamp and a threshold with an empty key.
    [ "$mailed" = "$prev" ] && prev=""
    [[ "$mailed" =~ ^[0-9]+$ ]] || { first=""; mailed=0; prev="$raw"; }
  else
    # Written by a version that stored the key alone.
    first=""; mailed=0; prev="$raw"
  fi

  if [ -z "$key" ]; then
    if [ -n "$prev" ]; then
      ALERT_ACTION=recovered
      ALERT_PREV="$prev"
      if [ -n "$first" ]; then
        ALERT_AGE=$(( now - first ))
        ALERT_AGE_H=$(alert_age_human "$ALERT_AGE")
      else
        ALERT_AGE_H="an unknown time"
      fi
      rm -f "$state"
    else
      ALERT_ACTION=ok
      rm -f "$state"
    fi
    return 0
  fi

  if [ "$key" != "$prev" ] || [ -z "$first" ]; then
    # A different set of things is wrong, so the clock starts again. Adding a
    # name to an existing outage is a new problem worth a mail, which is the
    # behaviour these checks already had.
    ALERT_ACTION=new
    ALERT_AGE=0
    ALERT_AGE_H="0m"
    echo "$now 0 $key" > "$state"
    return 0
  fi

  ALERT_AGE=$(( now - first ))
  ALERT_AGE_H=$(alert_age_human "$ALERT_AGE")
  local crossed
  crossed=$(_alert_threshold_for "$ALERT_AGE")
  if [ "$crossed" -gt "${mailed:-0}" ]; then
    ALERT_ACTION=escalate
    echo "$first $crossed $key" > "$state"
  else
    ALERT_ACTION=quiet
    echo "$first ${mailed:-0} $key" > "$state"
  fi
}

# Self test. Run this file directly rather than sourcing it.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  fail=0
  s=$(mktemp)
  check() { # what expected actual
    if [ "$2" = "$3" ]; then echo "  ok    $1"; else echo "  FAIL  $1: expected '$2', got '$3'"; fail=1; fi
  }

  echo "alert-state self test"

  rm -f "$s"; alert_decide "$s" ""
  check "healthy with no history is silent" "ok" "$ALERT_ACTION"

  alert_decide "$s" "a b"
  check "a new problem mails" "new" "$ALERT_ACTION"

  alert_decide "$s" "a b"
  check "the same problem straight after is quiet" "quiet" "$ALERT_ACTION"

  # Wind the clock back by rewriting first_seen.
  age_to() { echo "$(( $(date -u +%s) - $1 )) $2 $3" > "$s"; }

  age_to 21599 0 "a b"; alert_decide "$s" "a b"
  check "just under six hours stays quiet" "quiet" "$ALERT_ACTION"

  age_to 21601 0 "a b"; alert_decide "$s" "a b"
  check "past six hours escalates" "escalate" "$ALERT_ACTION"

  alert_decide "$s" "a b"
  check "and does not escalate twice for the same threshold" "quiet" "$ALERT_ACTION"

  age_to 86401 21600 "a b"; alert_decide "$s" "a b"
  check "past a day escalates again" "escalate" "$ALERT_ACTION"

  age_to 90000 86400 "a b"; alert_decide "$s" "a b"
  check "later the same day is quiet" "quiet" "$ALERT_ACTION"

  age_to 172801 86400 "a b"; alert_decide "$s" "a b"
  check "the second day escalates" "escalate" "$ALERT_ACTION"

  age_to 40000 21600 "a b"; alert_decide "$s" "a b c"
  check "a name joining is a new problem" "new" "$ALERT_ACTION"

  age_to 62000 21600 "a b"; alert_decide "$s" ""
  check "clearing reports recovery" "recovered" "$ALERT_ACTION"
  check "  with the set that cleared" "a b" "$ALERT_PREV"
  check "  and how long it lasted" "17h 13m" "$ALERT_AGE_H"

  alert_decide "$s" ""
  check "and does not report recovery twice" "ok" "$ALERT_ACTION"

  # A state file from before this existed.
  echo "x y" > "$s"; alert_decide "$s" "x y"
  check "a legacy state file is adopted, not re-mailed as new" "new" "$ALERT_ACTION"

  echo "x y" > "$s"; alert_decide "$s" ""
  check "a legacy state file still reports recovery" "recovered" "$ALERT_ACTION"
  check "  naming the set that cleared" "x y" "$ALERT_PREV"
  check "  and saying the age is unknown rather than inventing one"         "an unknown time" "$ALERT_AGE_H"

  # backup-check.sh used to write "ok <epoch>" into its state file and never
  # read it back. Pointed at that file, this reads the line as a problem key and
  # mails a recovery for an outage that never happened, which it did once on
  # 2026-09-01. A caller changing its state format changes its filename; the
  # helper cannot tell one caller's history from a key and should not try.
  echo "ok 1788302718" > "$s"; alert_decide "$s" ""
  check "a foreign state format is read as a key, so callers must not share one"         "recovered" "$ALERT_ACTION"

  check "durations read as time" "2d 3h" "$(alert_age_human 183600)"
  check "  and minutes under the hour" "45m" "$(alert_age_human 2700)"

  rm -f "$s"
  [ "$fail" = "0" ] && echo "self test passed" || echo "self test FAILED"
  exit "$fail"
fi
