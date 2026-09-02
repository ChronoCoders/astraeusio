#!/bin/bash
# Restricts the Docker-published web ports to Cloudflare's ranges.
#
# Why this exists and ufw is not enough: ufw already carries a complete
# Cloudflare allow list for 80 and 443, and it does nothing for this site.
# Docker publishes those ports by DNAT in PREROUTING, so the traffic traverses
# FORWARD and never reaches ufw's INPUT chain. On 2026-08-12 the origin answered
# a request from an address in no Cloudflare range with the full application
# HTML. DOCKER-USER is the first chain in FORWARD and is the only
# place a rule can apply to a published container port.
#
# Ranges change, so they are fetched, never hardcoded. Refreshed by:
#   - systemd cf-firewall.service at boot, ordered after docker
#   - cron weekly through cron-run.sh, so a failed refresh raises an alert
#
# Fails without touching the firewall if the fetch looks wrong. A bad list must
# not be able to lock Cloudflare out or open the port to everyone.
set -uo pipefail

IFACE=${CF_FW_IFACE:-eth0}
PORTS=${CF_FW_PORTS:-80,443}
V4_URL=${CF_FW_V4_URL:-https://www.cloudflare.com/ips-v4}
V6_URL=${CF_FW_V6_URL:-https://www.cloudflare.com/ips-v6}
DRY=${CF_FW_DRY_RUN:-0}

log() { echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cf-firewall: $*"; }
die() { echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cf-firewall: FAILED: $*" >&2; exit 1; }

tmp=$(mktemp -d /tmp/cffw.XXXXXX)
trap 'rm -rf "$tmp"' EXIT

# ── fetch ─────────────────────────────────────────────────────────────────────
curl -fsS --max-time 20 "$V4_URL" -o "$tmp/v4" || die "could not fetch $V4_URL"
curl -fsS --max-time 20 "$V6_URL" -o "$tmp/v6" || die "could not fetch $V6_URL"

# ── validate, before anything is applied ──────────────────────────────────────
# Every line must be a CIDR, and the counts must be plausible. Cloudflare has
# published 15 v4 and 7 v6 ranges for years; a wildly different number means the
# endpoint returned something else, such as an error page or a captive portal.
grep -qE '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+/[0-9]+$' "$tmp/v4" || die "v4 list has no CIDR lines"
if grep -vqE '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+/[0-9]+$' "$tmp/v4"; then die "v4 list has a non-CIDR line"; fi
if grep -vqE '^[0-9a-fA-F:]+/[0-9]+$' "$tmp/v6"; then die "v6 list has a non-CIDR line"; fi

n4=$(grep -c . "$tmp/v4"); n6=$(grep -c . "$tmp/v6")
if [ "$n4" -lt 5 ] || [ "$n4" -gt 60 ]; then die "implausible v4 range count: $n4"; fi
if [ "$n6" -lt 1 ] || [ "$n6" -gt 40 ]; then die "implausible v6 range count: $n6"; fi
log "fetched $n4 v4 and $n6 v6 ranges"

# ── build the chain, applied atomically by iptables-restore ───────────────────
# -n keeps every other chain untouched; the ":DOCKER-USER - [0:0]" line replaces
# the contents of just this chain in one operation, so there is no window where
# the rules are half applied.
build() {
  local file=$1
  echo "*filter"
  echo ":DOCKER-USER - [0:0]"
  # Existing flows keep working, including the replies to container egress.
  echo "-A DOCKER-USER -m conntrack --ctstate ESTABLISHED,RELATED -j RETURN"
  # `|| [ -n "$cidr" ]` is load bearing: Cloudflare's list ends without a
  # trailing newline, and a plain `while read` silently drops that final range.
  # The dry run showed 14 of 15 before this was added.
  while read -r cidr || [ -n "$cidr" ]; do
    [ -n "$cidr" ] || continue
    echo "-A DOCKER-USER -i $IFACE -s $cidr -p tcp -m multiport --dports $PORTS -j RETURN"
  done < "$file"
  # Anything else arriving from the internet for those ports stops here.
  # Traffic on other ports and all container to container traffic falls through
  # untouched, which is why this is a targeted DROP and not a policy change.
  echo "-A DOCKER-USER -i $IFACE -p tcp -m multiport --dports $PORTS -j DROP"
  echo "COMMIT"
}

build "$tmp/v4" > "$tmp/rules4"
build "$tmp/v6" > "$tmp/rules6"

if [ "$DRY" = "1" ]; then
  log "dry run, would apply:"
  sed 's/^/    /' "$tmp/rules4"
  exit 0
fi

iptables-restore -n < "$tmp/rules4" || die "iptables-restore rejected the v4 rules"
log "applied $n4 v4 allow rules plus a default drop on $PORTS via $IFACE"

# IPv6 reaches the containers through docker-proxy in userland, which means it
# arrives on INPUT where ufw already filters it. The chain is still populated so
# the protection does not depend on that remaining true.
if ip6tables -S DOCKER-USER >/dev/null 2>&1; then
  ip6tables-restore -n < "$tmp/rules6" || die "ip6tables-restore rejected the v6 rules"
  log "applied $n6 v6 allow rules"
else
  log "no ip6tables DOCKER-USER chain, skipping v6 (ufw already filters it on INPUT)"
fi

log "done"
