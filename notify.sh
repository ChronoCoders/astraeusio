#!/bin/bash
# The one way anything on this host sends an alert.
#
# Every scheduled job used to end with `if command -v mail`. There is no mail
# binary, so backup-check.sh, poller-check.sh and cron-run.sh have been writing
# alerts to journald and stopping there. A backup failed for 23 hours and a
# solar wind feed died for 3 and both alerts were raised correctly and read by
# nobody. An alert that cannot reach a person is not an alert.
#
# So all of them call this instead. Resend over curl, key read from
# backend/.env, one place to rotate. curl and not python: api.resend.com sits
# behind Cloudflare, which answers "error code: 1010" to urllib's user agent,
# and a send that fails silently is worse than no send at all.
#
# Usage:  notify.sh <subject>            # body on stdin
#         notify.sh <subject> <body>
#
# Exit 0 if the mail was accepted, 1 otherwise. Callers should not gate their
# own exit status on this; a failure to notify is logged, not fatal.
set -uo pipefail

SUBJECT=${1:?usage: notify.sh <subject> [body]}
if [ $# -ge 2 ]; then BODY=$2; else BODY=$(cat); fi

ENVF=${NOTIFY_ENV:-/opt/astraeusio/backend/.env}
ALERT_TO=${NOTIFY_TO:-altug@bytus.io}
FROM=${NOTIFY_FROM:-Astraeus <noreply@astraeusio.com>}
SEND=${NOTIFY_SEND:-1}
HOST=$(hostname)

if [ "$SEND" != "1" ]; then
  echo "notify: suppressed (NOTIFY_SEND=$SEND): $SUBJECT"
  exit 0
fi

KEY=$(grep -E '^RESEND_API_KEY=' "$ENVF" 2>/dev/null | cut -d= -f2- | tr -d '\r\n')
if [ -z "$KEY" ]; then
  echo "notify: no RESEND_API_KEY in $ENVF, cannot send: $SUBJECT" >&2
  logger -t astraeusio-notify -p daemon.err "cannot send alert, no key: $SUBJECT"
  exit 1
fi

PAYLOAD=$(mktemp /tmp/notify.XXXXXX)
RESP=$(mktemp /tmp/notify-resp.XXXXXX)
trap 'rm -f "$PAYLOAD" "$RESP"' EXIT

python3 - "$SUBJECT" "$BODY" "$ALERT_TO" "$FROM" "$HOST" > "$PAYLOAD" <<'PY'
import json, sys
subject, body, to, frm, host = sys.argv[1:6]
esc = body.replace("&", "&amp;").replace("<", "&lt;")
print(json.dumps({
    "from": frm,
    "to": [to],
    "subject": subject,
    "html": f"<pre>{esc}</pre><p style='color:#888'>host: {host}</p>",
}))
PY

HTTP=$(curl -s -o "$RESP" -w '%{http_code}' --max-time 20 \
  -X POST https://api.resend.com/emails \
  -H "Authorization: Bearer $KEY" \
  -H 'Content-Type: application/json' \
  --data @"$PAYLOAD")

if [ "$HTTP" = "200" ]; then
  echo "notify: sent ($SUBJECT)"
  exit 0
fi

echo "notify: FAILED http=$HTTP $(head -c 200 "$RESP")" >&2
logger -t astraeusio-notify -p daemon.err "alert send failed http=$HTTP: $SUBJECT"
exit 1
