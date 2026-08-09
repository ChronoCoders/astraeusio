#!/bin/bash
# Deploy Astraeus on the production host. Run this on the server, from the
# repository root, as root.
#
#   ./deploy.sh                     deploy, rebuilding only what changed
#   ./deploy.sh --dry-run           show what it would do, change nothing
#   ./deploy.sh --services "ml"     override the service selection
#   ./deploy.sh --all               rebuild every service
#   ./deploy.sh --rollback <sha>    retag images from a previous deploy and restart
#
# Three things this exists to prevent, each of which has happened:
#
#   1. `docker compose build` moves :latest to a new image and buildx does not
#      keep the previous one as a dangling image, so the old image is gone and
#      rollback needs a rebuild from source. Images are tagged with their commit
#      sha before and after the build so a rollback is a retag.
#
#   2. Rebuilding from habit rather than from the diff. A deploy once skipped ml
#      because "only Rust and React changed", but ml/serve.py had changed too and
#      the backend depends on a field that change added to the ML health
#      response. Every forecast returned 503 until ml was rebuilt separately.
#      Services are selected from the actual diff.
#
#   3. Deploying with no recent rollback point. This refuses to run if the newest
#      local backup is stale. It does not take one itself: backup.sh names its
#      artifact by date, so calling it mid deploy would overwrite the morning
#      backup with a post migration one and destroy the thing being relied on.
set -euo pipefail

REPO=/opt/astraeusio
BACKUP_DIR=$REPO/backups
BACKUP_MAX_AGE_H=30
SERVICES_ALL="backend frontend ml"

DRY=0
FORCE_ALL=0
OVERRIDE=""
ROLLBACK=""

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run)   DRY=1; shift ;;
    --all)       FORCE_ALL=1; shift ;;
    --services)  OVERRIDE="${2:-}"; shift 2 ;;
    --rollback)  ROLLBACK="${2:-}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

cd "$REPO"
log() { echo "$(date -u +%H:%M:%S)  $*"; }
run() { if [ "$DRY" = "1" ]; then echo "   would run: $*"; else "$@"; fi; }

# ── Rollback ──────────────────────────────────────────────────────────────────

if [ -n "$ROLLBACK" ]; then
  log "rollback to $ROLLBACK"
  for s in $SERVICES_ALL; do
    docker image inspect "astraeusio-$s:$ROLLBACK" > /dev/null 2>&1 \
      || { echo "no image astraeusio-$s:$ROLLBACK on this host" >&2; exit 1; }
  done
  for s in $SERVICES_ALL; do
    run docker tag "astraeusio-$s:$ROLLBACK" "astraeusio-$s:latest"
  done
  run docker compose up -d
  log "rolled back. The working tree is still at $(git rev-parse --short HEAD);"
  log "check out $ROLLBACK separately if the source needs to match."
  exit 0
fi

# ── Preflight ─────────────────────────────────────────────────────────────────

log "preflight"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "tracked files are modified. Commit or restore them first:" >&2
  git status --short --untracked-files=no >&2
  exit 1
fi
echo "   tracked tree clean"

latest_backup=$(ls -t "$BACKUP_DIR"/astraeus_*.duckdb 2>/dev/null | head -n 1 || true)
if [ -z "$latest_backup" ]; then
  echo "no backup in $BACKUP_DIR. Run backup.sh before deploying." >&2
  exit 1
fi
age_h=$(( ( $(date +%s) - $(stat -c %Y "$latest_backup") ) / 3600 ))
if [ "$age_h" -gt "$BACKUP_MAX_AGE_H" ]; then
  echo "newest backup $(basename "$latest_backup") is ${age_h}h old, limit ${BACKUP_MAX_AGE_H}h." >&2
  echo "Run backup.sh before deploying." >&2
  exit 1
fi
echo "   rollback backup: $(basename "$latest_backup"), ${age_h}h old"

OLD=$(git rev-parse --short HEAD)
echo "   deployed commit: $OLD"

# ── Tag what is running, before anything can move :latest ─────────────────────

log "tagging the running images as $OLD"
for s in $SERVICES_ALL; do
  if id=$(docker inspect "astraeusio-$s-1" --format '{{.Image}}' 2>/dev/null); then
    run docker tag "$id" "astraeusio-$s:$OLD"
    echo "   astraeusio-$s:$OLD"
  else
    echo "   astraeusio-$s-1 is not running, nothing to tag"
  fi
done

# ── Fetch and choose services from the diff ───────────────────────────────────

# Always fetch, including on a dry run. It touches no working tree file, and
# without it the service selection below would be computed against a stale ref.
log "fetching"
git fetch origin
NEW=$(git rev-parse --short origin/main)

if [ "$OLD" = "$NEW" ]; then
  echo "   already at $NEW, nothing to pull"
else
  echo "   $OLD to $NEW"
  git --no-pager log --oneline "HEAD..origin/main" | sed 's/^/     /'
fi

if [ -n "$OVERRIDE" ]; then
  SERVICES="$OVERRIDE"
  echo "   services overridden: $SERVICES"
elif [ "$FORCE_ALL" = "1" ] || [ "$OLD" = "$NEW" ]; then
  SERVICES="$SERVICES_ALL"
  echo "   services: all"
else
  changed=$(git diff --name-only "HEAD..origin/main")
  SERVICES=""
  echo "$changed" | grep -q '^backend/'  && SERVICES="$SERVICES backend"
  echo "$changed" | grep -q '^frontend/' && SERVICES="$SERVICES frontend"
  echo "$changed" | grep -q '^ml/'       && SERVICES="$SERVICES ml"
  # A compose change alters every container, so rebuild and recreate all of them.
  echo "$changed" | grep -q '^docker-compose.yml$' && SERVICES="$SERVICES_ALL"
  SERVICES=$(echo "$SERVICES" | xargs || true)
  [ -z "$SERVICES" ] && SERVICES="$SERVICES_ALL"
  echo "   changed paths select: $SERVICES"
fi

# ── Pull, build, tag the result, bring it up ──────────────────────────────────

log "pulling"
run git pull --ff-only
if [ "$DRY" = "0" ] && [ "$(git rev-parse --short HEAD)" != "$NEW" ]; then
  echo "HEAD is $(git rev-parse --short HEAD) after the pull, expected $NEW" >&2
  exit 1
fi

log "building: $SERVICES"
# shellcheck disable=SC2086
run docker compose build $SERVICES

log "tagging the new images as $NEW"
for s in $SERVICES; do
  run docker tag "astraeusio-$s:latest" "astraeusio-$s:$NEW"
  echo "   astraeusio-$s:$NEW"
done

log "starting"
run docker compose up -d

if [ "$DRY" = "1" ]; then
  log "dry run finished, nothing was changed"
  exit 0
fi

# ── Verify ────────────────────────────────────────────────────────────────────

log "waiting for the backend to bind"
deadline=$(( $(date +%s) + 600 ))
ready=0
while [ "$(date +%s)" -lt "$deadline" ]; do
  if docker logs astraeusio-backend-1 2>&1 | grep -q 'listening on'; then ready=1; break; fi
  if [ "$(docker inspect -f '{{.State.Running}}' astraeusio-backend-1 2>/dev/null)" != "true" ]; then
    echo "backend container is not running" >&2
    docker logs --tail 40 astraeusio-backend-1 >&2
    exit 1
  fi
  sleep 3
done
[ "$ready" = "1" ] || { echo "backend did not bind within 10 minutes" >&2; exit 1; }
echo "   backend bound"

fail=0
code=$(curl -sSk -o /dev/null -w '%{http_code}' --max-time 15 https://127.0.0.1/ || true)
echo "   frontend https: $code"; [ "$code" = "200" ] || fail=1
code=$(curl -sSk -o /dev/null -w '%{http_code}' --max-time 15 https://127.0.0.1/api/health || true)
echo "   backend api:    $code"; [ "$code" = "200" ] || fail=1

# The ML contract the backend depends on. A 200 alone does not prove the image
# is the matching version, which is how a stale ml image went unnoticed.
if echo "$SERVICES" | grep -q ml; then
  seq=$(docker run --rm --network astraeusio_default curlimages/curl:latest \
          -sS --max-time 8 http://ml:8000/health 2>/dev/null \
        | python3 -c 'import sys,json; print(json.load(sys.stdin).get("seq_len","ABSENT"))' 2>/dev/null || echo ABSENT)
  echo "   ml seq_len:     $seq"
  [ "$seq" = "ABSENT" ] && fail=1
fi

errs=$(docker logs --since 5m astraeusio-backend-1 2>&1 | grep -c ERROR || true)
echo "   backend ERROR lines in the last 5m: $errs"

if [ "$fail" != "0" ]; then
  echo >&2
  echo "DEPLOY VERIFICATION FAILED. Roll back with:" >&2
  echo "  ./deploy.sh --rollback $OLD" >&2
  exit 1
fi

log "deployed $OLD to $NEW, services: $SERVICES"
log "rollback if needed: ./deploy.sh --rollback $OLD"
