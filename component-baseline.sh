#!/bin/bash
# Which components the backend is expected to publish. Sourced, not run.
#
# The gap this closes: both checks that read /api/health enumerate from the
# payload. component-check.sh loops over the components it was handed and asks
# whether each is fresh. poller-check.sh looks up a component name among the
# ones it was handed. Neither can see a component that stopped being published
# at all, because a list built from what has spoken cannot contain what has gone
# silent. noaa_alerts is the mirror image: it was written for weeks and read by
# nobody, so its uptime strip was empty from the day it was added until
# 2026-09-01, and no check noticed because no check held a list to compare.
#
# The published set is a property of the deployed binary and not of the data.
# routes.rs builds it from health_components() in db.rs, so it cannot differ
# between two runs against the same container. A name leaving it is therefore
# always a deploy and never a blip, which is why one raise is enough and there
# is no debouncing here.
#
#   baseline_compare <file> <current_set>
#
# `current_set` is the component names in the payload, space separated. Order
# does not matter. Sets:
#
#   BASELINE_STATE  same | changed | unset | empty
#   BASELINE_GONE   names the baseline has that the payload no longer carries
#   BASELINE_NEW    names the payload has that the baseline does not
#
# It never writes, and that is the whole of the design. A check that updates its
# own baseline turns a component going quiet into the new normal on the next
# run: the alarm fires once, into a cycle nobody was watching, and is gone
# forever. The only writer is baseline_accept, and its only caller is an
# operator running `component-check.sh --accept-components` by hand.
#
# So a removal is a two step action. The deploy drops the component, the check
# raises, and the raise clears only when someone decides it should. An
# accidental removal cannot be waited out.

# Splitting a stored line into words is what these two do; the quoting warning
# is about the case where that is not intended, and here it is.
# shellcheck disable=SC2086

_baseline_norm() {  # normalises a set to sorted, unique, space separated
  printf '%s\n' $1 | awk 'NF' | sort -u | tr '\n' ' ' | sed 's/ *$//'
}

baseline_compare() {
  local file=$1 current=$2 recorded=""
  BASELINE_STATE=unset
  BASELINE_GONE=""
  BASELINE_NEW=""

  # An empty set from a payload that parsed is a fault of its own, and it must
  # not be read as "every component has gone", which would mail the whole list.
  if [ -z "$(_baseline_norm "$current")" ]; then
    BASELINE_STATE=empty
    return 0
  fi

  [ -f "$file" ] && recorded=$(head -n 1 "$file" 2>/dev/null)
  if [ -z "$(_baseline_norm "${recorded:-}")" ]; then
    BASELINE_STATE=unset
    return 0
  fi

  local a b
  a=$(printf '%s\n' $recorded | awk 'NF' | sort -u)
  b=$(printf '%s\n' $current | awk 'NF' | sort -u)
  BASELINE_GONE=$(comm -23 <(printf '%s\n' "$a") <(printf '%s\n' "$b") | tr '\n' ' ' | sed 's/ *$//')
  BASELINE_NEW=$(comm -13 <(printf '%s\n' "$a") <(printf '%s\n' "$b") | tr '\n' ' ' | sed 's/ *$//')

  if [ -z "$BASELINE_GONE" ] && [ -z "$BASELINE_NEW" ]; then
    BASELINE_STATE=same
  else
    BASELINE_STATE=changed
  fi
}

# The only writer. Refuses an empty set, so a parse that came back with nothing
# cannot be accepted as the new expectation and silence every later comparison.
baseline_accept() {
  local file=$1 current=$2 set
  set=$(_baseline_norm "$current")
  if [ -z "$set" ]; then
    echo "refusing to write an empty baseline" >&2
    return 1
  fi
  mkdir -p "$(dirname "$file")"
  {
    echo "$set"
    echo "# accepted $(date -u) by ${SUDO_USER:-${USER:-unknown}} on $(hostname)"
  } > "$file.new"
  mv "$file.new" "$file"
}

# Self test. Run this file directly rather than sourcing it.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  fail=0
  f=$(mktemp); rm -f "$f"
  check() {  # what expected actual
    if [ "$2" = "$3" ]; then echo "  ok    $1"; else echo "  FAIL  $1: expected '$2', got '$3'"; fail=1; fi
  }

  echo "component-baseline self test"

  baseline_compare "$f" "a b c"
  check "no baseline is unset, not a match" "unset" "$BASELINE_STATE"
  check "  and names nothing as gone" "" "$BASELINE_GONE"

  : > "$f"
  baseline_compare "$f" "a b c"
  check "an empty baseline file is also unset" "unset" "$BASELINE_STATE"

  # The constraint this file exists to hold: comparing must never write.
  rm -f "$f"
  baseline_compare "$f" "a b c"
  check "comparing with no baseline creates no file" "absent" "$([ -e "$f" ] && echo present || echo absent)"

  baseline_accept "$f" "c a b"
  check "accept stores the set sorted" "a b c" "$(head -n 1 "$f")"
  check "  and records who and when" "1" "$(sed -n '2p' "$f" | grep -c '^# accepted ')"

  before=$(cat "$f")
  baseline_compare "$f" "a b c"
  check "a matching set is the same" "same" "$BASELINE_STATE"
  check "  and comparing left the file alone" "$before" "$(cat "$f")"

  baseline_compare "$f" "c b a"
  check "order does not matter" "same" "$BASELINE_STATE"

  baseline_compare "$f" "a c"
  check "a component gone is a change" "changed" "$BASELINE_STATE"
  check "  naming it" "b" "$BASELINE_GONE"
  check "  and nothing as new" "" "$BASELINE_NEW"
  check "  without rewriting the baseline" "$before" "$(cat "$f")"

  baseline_compare "$f" "a b c d"
  check "a component added is a change" "changed" "$BASELINE_STATE"
  check "  naming it" "d" "$BASELINE_NEW"
  check "  and nothing as gone" "" "$BASELINE_GONE"
  check "  without rewriting the baseline" "$before" "$(cat "$f")"

  baseline_compare "$f" "a b d"
  check "one leaving and one joining reports both, gone" "c" "$BASELINE_GONE"
  check "  and new" "d" "$BASELINE_NEW"

  # A parsed payload with no components must not read as a total wipe.
  baseline_compare "$f" ""
  check "an empty payload is its own state" "empty" "$BASELINE_STATE"
  check "  and does not claim everything is gone" "" "$BASELINE_GONE"
  baseline_compare "$f" "   "
  check "whitespace is empty too" "empty" "$BASELINE_STATE"

  baseline_accept "$f" "" 2>/dev/null && r=wrote || r=refused
  check "accept refuses an empty set" "refused" "$r"
  check "  leaving the baseline as it was" "$before" "$(cat "$f")"

  rm -f "$f" "$f.new"
  [ "$fail" = "0" ] && echo "self test passed" || echo "self test FAILED"
  exit "$fail"
fi
