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

# No sccache here, and that is a measured decision rather than an omission.
#
# `reqwest` is a build dependency of `libduckdb-sys`, so bumping any crate
# beneath it gives that build script a new fingerprint and recompiles the bundled
# DuckDB amalgamation, about 10 minutes and 6 GB of objects. sccache looks like
# the answer and is not: cc 1.2.60 does pick it up through `RUSTC_WRAPPER` in
# `rustc_wrapper_fallback`, the wrapper does reach `cl.exe`, and sccache then
# refuses the work. Measured on 2026-11-24 with `RUSTC_WRAPPER=sccache` set for a
# full run: four C++ calls, zero cached, `Non-cacheable reasons: multiple input
# files 4`. That build script passes many `.cpp` files to one `cl.exe`
# invocation, and sccache cannot cache a compile with more than one input.
#
# A single file probe crate cached correctly in the same session, which is the
# trap: it proves the wrapper works and says nothing about this build.
#
# The lever that would change it is enabling cc's `parallel` feature so each file
# gets its own invocation, which means adding `cc` to this crate's build
# dependencies purely to unify a feature onto `libduckdb-sys`. Not done: it puts
# a hack in the manifest that the production image build also reads.

# Leave the machine usable while the gate runs.
#
# cargo runs one job per core and `cc` takes its parallelism from the same
# jobserver through NUM_JOBS. On 2026-11-24 a cold rebuild on this 4 core laptop
# put 15 compilers on those 4 cores, the desktop stopped responding, and the run
# had to be killed half way, which is a worse outcome than a slower gate. Two
# cores stay free unless the caller sets the variable.
if [ -z "${CARGO_BUILD_JOBS:-}" ]; then
  cores=$(nproc 2>/dev/null || echo 2)
  if [ "$cores" -gt 2 ]; then
    export CARGO_BUILD_JOBS=$((cores - 2))
  fi
fi

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
# Node's own test runner over the pure helpers in src/lib. No vitest and no
# jsdom: the repository already sets "type": "module", so these import directly
# and nothing was added to package.json to run them.
step "frontend test"  frontend node --test src/lib/
# The published agent-skill hashes. A Rust test cannot reach
# frontend/public, and the frontend test runner covers src/lib only, so this
# manifest had nothing checking it and all four values were wrong.
step "skill hashes"   .        bash skill-hashes.sh
# Standard library unittest, discovered from the repository root exactly as
# ml/test_serve.py documents. No pytest, no dev requirements file.
step "advisory tests" .        python -m unittest test_advisory_check
step "advisories"     .        python advisory-check.py
step "ml test"        .        python -m unittest discover -s ml -p "test_*.py"
step "naming rule"              .        bash scripts/lib/naming.sh --gate
step "control: naming detector" .        bash scripts/lib/naming.sh --self-test
# Counts three classes, not two: U+2013 en dash, U+2212 minus sign and U+2014 em
# dash. U+2212 is in scope because a reader cannot tell it from an en dash on
# screen. The target is zero, swept from a measured 58 on 2026-10-04, and the
# step name carries that so nobody has to open the script to learn which
# characters it counts. The control matters more than usual here: the first
# version assembled its patterns as an escape around a variable, which this bash
# leaves as the six ASCII bytes of the escape rather than the character, while
# the same escape written literally expands. It therefore searched for a string
# no file contains and passed on a tree holding all 58, and its own self test
# passed alongside it because it planted the identical wrong string.
step "dash sweep: U+2013 U+2212 U+2014, target 0" . bash scripts/lib/dashes.sh --gate
step "control: dash detector"   .        bash scripts/lib/dashes.sh --self-test
# The Rust version lives in backend/rust-toolchain.toml and nowhere else. The
# Dockerfile used to carry its own, and the two drifted five minor versions
# apart without anything noticing. The check refuses any version there, not
# today's, so the rule holds whatever number someone reaches for.
step "toolchain pin"            .        bash scripts/lib/toolchain.sh --gate
step "control: toolchain pin"   .        bash scripts/lib/toolchain.sh --self-test


echo
printf '%s\n' "${RESULTS[@]}" | sed 's/^/  /'
echo
if [ "$FAILED" -eq 0 ]; then
  echo "gate: PASS, $STEPS of $STEPS"
else
  echo "gate: FAIL, $FAILED of $STEPS failed"
fi
exit "$FAILED"
