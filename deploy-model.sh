#!/bin/bash
# Place a trained checkpoint on the shared volume and prove it is the one serving.
#
# The model is not in version control and never has been: `.gitignore` excludes
# `ml/models/` because a 230 KB binary does not belong in a diff. The image
# bundles whatever happened to be on the machine that built it, and the ml
# container's start command copies that bundled file to the volume only when no
# file is already there, so a rebuild never replaces a deployed model. Until this
# script existed, shipping a model meant copying a file by hand and there was no
# record of which training run produced the one in production.
#
# What this fixes, in the order the questions get asked:
#
#   how the file gets there    scp to the host, then this script, which verifies
#                              the hash after transfer and places it by content
#                              address rather than overwriting in place
#   which run it came from     the checkpoint carries its own provenance and
#                              ml /health reports the sha256 of the file it
#                              loaded, so the running model is identifiable from
#                              outside the container
#   how rollback works         every deployed checkpoint stays on the volume
#                              under its own hash. A deploy that does not end
#                              with /health reporting the expected sha restores
#                              the previous file and restarts, before this script
#                              returns non-zero
#
# Usage:  ./deploy-model.sh /path/to/kp_lstm.pt
#         ./deploy-model.sh --rollback <sha12>
#         ./deploy-model.sh --list
set -uo pipefail

VOLUME=${MODEL_VOLUME_DIR:-/var/lib/docker/volumes/astraeusio_data/_data/models}
ACTIVE="$VOLUME/kp_lstm.pt"
ML_URL=${ML_HEALTH_URL:-http://127.0.0.1:8000/health}
COMPOSE_DIR=${COMPOSE_DIR:-/opt/astraeusio}
WAIT_SECS=${MODEL_DEPLOY_WAIT:-90}

log() { echo "$(date -u '+%H:%M:%S')  $*"; }
fail() { echo "$(date -u '+%H:%M:%S')  FAILED: $*" >&2; exit 1; }

sha_of() { sha256sum "$1" | cut -c1-64; }

# The health probe runs inside the ml container, because port 8000 is
# deliberately not published to the host.
ml_health() {
  docker compose -f "$COMPOSE_DIR/docker-compose.yml" exec -T ml \
    python3 -c "import json,urllib.request;print(json.load(urllib.request.urlopen('$ML_URL',timeout=5)).get('model_sha256',''))" \
    2>/dev/null | tr -d '\r\n'
}

restart_ml() {
  docker compose -f "$COMPOSE_DIR/docker-compose.yml" restart ml >/dev/null 2>&1 \
    || fail "could not restart the ml service"
}

# Waits for ml to come back and report the sha we expect. Returns 1 on either a
# timeout or a mismatch, and the caller decides what to do about it.
await_sha() {
  local want=$1 seen=""
  for _ in $(seq 1 "$WAIT_SECS"); do
    seen=$(ml_health)
    [ "$seen" = "$want" ] && return 0
    sleep 1
  done
  echo "  /health reports '${seen:-nothing}', expected '$want'" >&2
  return 1
}

mkdir -p "$VOLUME"

case "${1:-}" in
  --list)
    log "checkpoints on the volume"
    active_sha=""
    [ -f "$ACTIVE" ] && active_sha=$(sha_of "$ACTIVE")
    for f in "$VOLUME"/kp_lstm_*.pt; do
      [ -e "$f" ] || continue
      s=$(sha_of "$f")
      marker=""
      [ "$s" = "$active_sha" ] && marker="  <- active"
      printf '  %s  %s  %s%s\n' "${s:0:12}" "$(date -u -r "$f" '+%Y-%m-%d %H:%M')" "$(basename "$f")" "$marker"
    done
    exit 0
    ;;
  --rollback)
    want12=${2:-}
    [ -n "$want12" ] || fail "usage: $0 --rollback <sha12>"
    target="$VOLUME/kp_lstm_$want12.pt"
    [ -f "$target" ] || fail "no checkpoint $want12 on the volume, try --list"
    want=$(sha_of "$target")
    log "rolling back to $want12"
    cp "$target" "$ACTIVE" || fail "could not place the checkpoint"
    restart_ml
    await_sha "$want" || fail "rollback did not take: ml is not serving $want12"
    log "rolled back, ml is serving $want12"
    exit 0
    ;;
esac

SRC=${1:-}
[ -n "$SRC" ] || fail "usage: $0 /path/to/kp_lstm.pt"
[ -f "$SRC" ] || fail "no such file: $SRC"

NEW=$(sha_of "$SRC")
NEW12=${NEW:0:12}

OLD=""
if [ -f "$ACTIVE" ]; then
  OLD=$(sha_of "$ACTIVE")
  if [ "$OLD" = "$NEW" ]; then
    log "ml is already serving $NEW12, nothing to do"
    exit 0
  fi
  # Keep the outgoing model addressable before anything moves, so the rollback
  # path exists even if the rest of this script dies.
  if [ ! -f "$VOLUME/kp_lstm_${OLD:0:12}.pt" ]; then
    cp "$ACTIVE" "$VOLUME/kp_lstm_${OLD:0:12}.pt" || fail "could not preserve the current model"
    log "preserved the current model as ${OLD:0:12}"
  fi
fi

log "placing $NEW12"
cp "$SRC" "$VOLUME/kp_lstm_$NEW12.pt" || fail "could not copy the checkpoint onto the volume"
[ "$(sha_of "$VOLUME/kp_lstm_$NEW12.pt")" = "$NEW" ] || fail "the copy on the volume does not match the source hash"
cp "$VOLUME/kp_lstm_$NEW12.pt" "$ACTIVE" || fail "could not make $NEW12 active"

restart_ml
if await_sha "$NEW"; then
  log "deployed, ml is serving $NEW12"
  exit 0
fi

echo "  the new model did not come up, restoring" >&2
if [ -n "$OLD" ]; then
  cp "$VOLUME/kp_lstm_${OLD:0:12}.pt" "$ACTIVE"
  restart_ml
  if await_sha "$OLD"; then
    fail "$NEW12 did not come up. Rolled back to ${OLD:0:12}, which is serving."
  fi
  fail "$NEW12 did not come up AND the rollback to ${OLD:0:12} did not either. ml needs a look."
fi
fail "$NEW12 did not come up and there was no previous model to restore."
