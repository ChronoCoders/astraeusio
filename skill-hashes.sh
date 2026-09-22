#!/bin/bash
# Checks that every published agent-skill hash matches the file it names.
#
# `frontend/public/.well-known/agent-skills/index.json` carries a sha256 per
# skill so an agent fetching a SKILL.md can tell it got what the index promised.
# On 2026-09-22 all four were wrong, and had been for long enough that nobody
# could say when they diverged. Nothing checked them: the Rust test that guards
# the backlog cannot reach frontend/public, and no frontend test existed at all
# until the same day.
#
#   skill-hashes.sh            check, exit 1 on any mismatch
#   skill-hashes.sh --write    recompute and write them back
#
# A published integrity manifest that nobody verifies is worse than none: it
# invites a consumer to trust a value that drifts silently every time a skill
# file is edited.
set -uo pipefail

DIR=${SKILL_DIR:-$(dirname "$0")/frontend/public/.well-known/agent-skills}
INDEX="$DIR/index.json"
WRITE=0
[ "${1:-}" = "--write" ] && WRITE=1

[ -f "$INDEX" ] || { echo "no index at $INDEX" >&2; exit 2; }

# The carriage returns are stripped because python3 on Windows writes CRLF in
# text mode, and a name carrying a trailing CR builds a path that cannot exist.
# The check then called every skill MISSING and exited non-zero for a reason
# that had nothing to do with the hashes it exists to verify, which is a control
# that proves nothing.
mapfile -t NAMES < <(python3 -c "
import json,sys
for s in json.load(open(sys.argv[1], encoding='utf-8'))['skills']:
    print(s['name'])
" "$INDEX" | tr -d '\r')

# A floor. An index that parsed to nothing would otherwise report every hash
# correct, which is the shape of a check that cannot fail.
if [ "${#NAMES[@]}" -lt 1 ]; then
  echo "index.json declares no skills; nothing was checked" >&2
  exit 2
fi

fail=0
for name in "${NAMES[@]}"; do
  file="$DIR/$name/SKILL.md"
  if [ ! -f "$file" ]; then
    echo "  MISSING  $name: no $file"
    fail=1
    continue
  fi
  actual=$(sha256sum "$file" | cut -d' ' -f1)
  declared=$(python3 -c "
import json,sys
for s in json.load(open(sys.argv[1], encoding='utf-8'))['skills']:
    if s['name'] == sys.argv[2]:
        print(s.get('sha256',''))
" "$INDEX" "$name" | tr -d '\r')
  if [ "$actual" = "$declared" ]; then
    printf '  ok       %-22s %s\n' "$name" "${actual:0:16}"
  else
    printf '  MISMATCH %-22s declared %s actual %s\n' "$name" "${declared:0:16}" "${actual:0:16}"
    fail=1
  fi
done

if [ "$WRITE" = "1" ]; then
  python3 - "$INDEX" "$DIR" <<'PY'
import hashlib, io, json, os, sys
index, base = sys.argv[1], sys.argv[2]
raw = io.open(index, encoding="utf-8", newline="").read()
doc = json.loads(raw)
for s in doc["skills"]:
    p = os.path.join(base, s["name"], "SKILL.md")
    if not os.path.exists(p):
        continue
    new = hashlib.sha256(io.open(p, "rb").read()).hexdigest()
    old = s.get("sha256", "")
    if old and old != new:
        # A targeted replace, so only the hash moves and the file's formatting,
        # key order and line endings are untouched.
        assert raw.count(old) == 1, "%s appears %d times" % (old, raw.count(old))
        raw = raw.replace(old, new)
        print("  rewrote  %-22s %s -> %s" % (s["name"], old[:16], new[:16]))
json.loads(raw)
io.open(index, "w", encoding="utf-8", newline="").write(raw)
PY
  echo "  rewritten; re-run without --write to confirm"
  exit 0
fi

if [ "$fail" = "0" ]; then
  echo "  all ${#NAMES[@]} published hashes match"
else
  echo "  ${#NAMES[@]} skills checked, at least one mismatch"
fi
exit "$fail"
