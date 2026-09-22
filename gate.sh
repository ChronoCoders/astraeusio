#!/bin/bash
# The gate. Every check that has to pass before anything ships, in one place.
#
# It exists because a gate composed by hand is a gate that can be composed
# wrong. On 2026-09-21 commit 87c5188 went out reported as gated after clippy
# and cargo test were run and `cargo fmt --all --check` was not, and the tree
# had been made format clean two commits earlier on purpose. Nothing caught it
# because nothing was looking: the gate was three commands somebody remembered.
# A missing check leaves no trace, which is exactly the failure a script cannot
# have and a habit cannot avoid.
#
# Every step runs even after one fails. Stopping at the first failure hides the
# rest, and the whole point of running a gate is to learn everything that is
# wrong in one pass rather than one item per iteration.
#
# Run from the main checkout, not a worktree. The bundled DuckDB cannot build
# cold on this machine, so a worktree gate either takes an hour or fails for a
# reason that has nothing to do with the change.
set -uo pipefail

cd "$(dirname "$0")" || exit 1
ROOT=$(pwd)

FAILED=0
STEPS=0
declare -a RESULTS=()

# Runs one step, records its verdict, and never stops the run.
#
# Output goes to a file rather than the terminal so a failing step can show its
# tail without a passing one burying everything in cargo noise.
step() { # name dir command...
  local name=$1 dir=$2
  shift 2
  STEPS=$((STEPS + 1))
  local log
  log=$(mktemp)
  local start
  start=$(date +%s)
  ( cd "$ROOT/$dir" && "$@" ) > "$log" 2>&1
  local rc=$?
  local secs=$(( $(date +%s) - start ))
  if [ "$rc" -eq 0 ]; then
    printf 'PASS  %-22s %4ds\n' "$name" "$secs"
    RESULTS+=("PASS $name")
  else
    printf 'FAIL  %-22s %4ds  exit=%d\n' "$name" "$secs" "$rc"
    RESULTS+=("FAIL $name")
    FAILED=$((FAILED + 1))
    sed 's/^/        /' "$log" | tail -25
  fi
  rm -f "$log"
}

echo "gate: $(date -u '+%Y-%m-%dT%H:%M:%SZ')  $(git rev-parse --short HEAD 2>/dev/null || echo 'no git')"
echo

step "rust clippy"    backend  cargo clippy --all-targets -- -D warnings
# A separate gate from clippy, which does not see formatting at all. This is the
# check that was skipped.
step "rust fmt"       backend  cargo fmt --all --check
step "rust test"      backend  cargo test
# No --workspace flag: it errors on cargo-audit 0.22.1, and plain `cargo audit`
# already covers the whole workspace through Cargo.lock.
step "cargo audit"    backend  cargo audit
step "frontend lint"  frontend npx eslint src
step "frontend build" frontend npm run build
# Standard library unittest, discovered from the repository root exactly as
# ml/test_serve.py documents. No pytest, no dev requirements file.
step "ml test"        .        python -m unittest discover -s ml -p "test_*.py"

echo
printf '%s\n' "${RESULTS[@]}" | sed 's/^/  /'
echo
if [ "$FAILED" -eq 0 ]; then
  echo "gate: PASS, $STEPS of $STEPS"
else
  echo "gate: FAIL, $FAILED of $STEPS failed"
fi
exit "$FAILED"
