#!/bin/bash
# Is the site answering at all. Runs every five minutes.
#
# This owns "unreachable" and nothing else. component-check.sh owns "answering
# but a data source has gone stale", and defers to this one when the endpoint
# cannot be reached, so a single outage raises a single alarm.
#
# Delivery goes through notify.sh. This script used to carry its own copy of
# the Resend API key inline while being world readable, which is why the key
# now lives only in backend/.env at mode 600.
set -euo pipefail

URL=${HEALTHCHECK_URL:-https://astraeusio.com/api/health}
FLAG=${HEALTHCHECK_FLAG:-/tmp/astraeusio_down.flag}

# Exercisable without alerting. NOTIFY_SEND=0 silences every script on this
# host at once, since all of them send through notify.sh; this is the local
# switch, for exercising this check alone. Both exist because neither was set
# when a harness ran poller-check.sh against fixture logs and mailed 37 alerts
# in eleven seconds. The harness isolated state and data correctly and did not
# isolate the effect the script exists to produce.
SEND_MAIL=${HEALTHCHECK_MAIL:-1}

send_mail() {
  local subject=$1 body=$2
  if [ "$SEND_MAIL" != "1" ]; then
    echo "  (mail suppressed by HEALTHCHECK_MAIL=$SEND_MAIL: $subject)"
    return 0
  fi
  /opt/astraeusio/notify.sh "$subject" "$body" 2>&1 | sed 's/^/  /'
}

STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$URL" || echo "000")

if [ "$STATUS" != "200" ]; then
    # Alert once per outage; the flag stops a new mail every five minutes.
    if [ ! -f "$FLAG" ]; then
        touch "$FLAG"
        send_mail "[ALERT] astraeusio.com is DOWN" \
"Health check failed at $(date -u).

endpoint:    $URL
http status: $STATUS

The site did not answer. If it is answering but a data source is stale, that is
component-check.sh, not this." || true
        echo "$(date -u): DOWN (status=$STATUS), alert sent"
    else
        echo "$(date -u): DOWN (status=$STATUS), alert already sent"
    fi
else
    if [ -f "$FLAG" ]; then
        rm -f "$FLAG"
        send_mail "[RECOVERED] astraeusio.com is back UP" \
"Health check recovered at $(date -u).

endpoint: $URL" || true
        echo "$(date -u): UP, recovery alert sent"
    else
        echo "$(date -u): UP (status=$STATUS)"
    fi
fi
