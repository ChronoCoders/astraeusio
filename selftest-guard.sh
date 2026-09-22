#!/bin/bash
# Stops a selftest from mailing anybody. Sourced, not run.
#
# On 2026-09-22 a live component check was run by hand without NOTIFY_SEND=0.
# It happened to find nothing wrong, so nothing was sent, and that is the whole
# problem: the outcome depended on the state of the system rather than on
# anything in the harness. The rule already existed in writing, and writing it
# down had not stopped it, which is the argument for making it structural.
#
# Earlier and worse, a harness once ran poller-check.sh against fixture logs and
# mailed 37 alerts in eleven seconds. It isolated state and data correctly and
# never looked for the switch that controls the effect.
#
#   require_notify_suppressed <what>
#
# Unset is not an error: the guard sets NOTIFY_SEND=0 and exports it, so a
# selftest cannot mail by forgetting. NOTIFY_SEND=1 is an error, because that is
# somebody explicitly asking a selftest to send, and no selftest should. The
# distinction matters: setting it silently covers the forgetful case, refusing
# covers the case where a real check and a selftest were confused for each
# other.
#
# NOTIFY_SEND rather than the per-script switches. BACKUP_CHECK_MAIL and its
# siblings silence one script; a harness that invokes another one leaks straight
# past them. This is the switch every sender on the host goes through.
require_notify_suppressed() {
  local what=${1:-selftest}
  if [ "${NOTIFY_SEND:-}" = "1" ]; then
    echo "$what refuses to run with NOTIFY_SEND=1." >&2
    echo "  A selftest must not be able to send mail. Unset it, or set it to 0." >&2
    return 1
  fi
  export NOTIFY_SEND=0
  return 0
}
