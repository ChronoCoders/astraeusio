#!/usr/bin/env bash
# The frontend test step, run from frontend/.
#
# Node reads each argument to `node --test` as a glob pattern. Under Node 24 the
# old `node --test src/lib/` therefore matched the directory itself, Node tried to
# load it as a module, and the step failed without running a test (AUD-066). A
# pattern that matches nothing is no better: Node reports `tests 0` and exits 0.
#
# So the test files are enumerated from the tree here, passed to Node by name,
# and the set of files the runner reports having run must equal that list. No
# test files at all is a failure, and so is a file in which no test ran.
set -uo pipefail

mapfile -t files < <(find src -type f -regextype posix-extended \
  -regex '.*/([^/]*[._-]test|test[._-][^/]*)\.(js|mjs|cjs)' | sort)
if [ "${#files[@]}" -eq 0 ]; then
  echo "frontend test: no test files found under src, so nothing would run" >&2
  exit 1
fi

report=$(mktemp)
trap 'rm -f "$report"' EXIT

node --test \
  --test-reporter=spec --test-reporter-destination=stdout \
  --test-reporter=junit --test-reporter-destination="$report" \
  "${files[@]}"
rc=$?

expected=$(realpath "${files[@]}" | sort -u)
# A file that loads but declares no test still shows up, as one synthetic case
# named after its own path, and the directory in AUD-066 showed up the same way.
# Only cases with a name of their own count, so such a file is not reported.
reported=$(sed -nE 's/^\s*<testcase name="([^"]*)".* file="([^"]*)".*/\1\t\2/p' "$report" |
  awk -F '\t' '{ n = length($1); if (substr($2, length($2) - n) != "/" $1) print $2 }' | sort -u)
if [ "$expected" != "$reported" ]; then
  echo "frontend test: the runner did not report exactly the test files in the tree" >&2
  diff <(printf '%s\n' "$expected") <(printf '%s\n' "$reported") | sed 's/^/  /' >&2
  exit 1
fi

echo "frontend test: ${#files[@]} test file(s) in the tree, all ${#files[@]} reported by the runner"
exit "$rc"
