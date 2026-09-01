#!/bin/bash
# Snapshots the AOP observation to the host, hourly.
#
# aop.log lives inside the frontend container, so recreating that container
# throws away the evidence we are deliberately accumulating overnight. This
# copies the counts somewhere a container restart cannot reach, so the colo
# question can be answered tomorrow without waiting another night.
#
# Read only, and always exits 0: a missing container is not worth an alert.
set -uo pipefail
OUT=/var/log/astraeusio-aop-observe.log
STAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

if ! docker inspect astraeusio-frontend-1 >/dev/null 2>&1; then
  echo "$STAMP frontend container absent, nothing to sample" >> "$OUT"
  exit 0
fi

log=$(docker exec astraeusio-frontend-1 sh -c 'cat /var/log/nginx/aop.log 2>/dev/null' || true)
if [ -z "$log" ]; then
  echo "$STAMP aop.log empty or unreadable" >> "$OUT"
  exit 0
fi

cf=$(printf '%s\n' "$log" | grep -v 'ray=-' || true)
success=$(printf '%s\n' "$cf" | grep -c 'verify=SUCCESS' || true)
failed=$(printf '%s\n' "$cf" | grep -c 'verify=FAILED' || true)
none=$(printf '%s\n' "$cf" | grep -c 'verify=NONE' || true)
colos=$(printf '%s\n' "$cf" | grep -oE 'ray=[0-9a-f]+-[A-Z]+' | sed 's/.*-//' | sort -u | tr '\n' ',' | sed 's/,$//')
ncolos=$(printf '%s\n' "$cf" | grep -oE 'ray=[0-9a-f]+-[A-Z]+' | sed 's/.*-//' | sort -u | grep -c . || true)

echo "$STAMP cf_success=$success cf_failed=$failed cf_none=$none colos=$ncolos [$colos]" >> "$OUT"

# Keep the raw lines too, so a certificate that changes can be inspected later
# rather than only counted.
printf '%s\n' "$cf" | grep 'verify=SUCCESS' | tail -3 >> "${OUT}.samples"
exit 0
