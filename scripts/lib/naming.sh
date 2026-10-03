#!/usr/bin/env bash
# Naming rule checks, shared by this repository's gate and its git hooks so the
# two cannot drift apart.
#
# The token is written as a split character class, so this file never matches
# itself. self_test proves the class still matches the literal, which is what
# catches an edit that quietly turns every check into a pass.
#
# Usable two ways:
#   source it, then call naming_scan, naming_scan_paths, naming_staged
#   run it:  naming.sh --self-test | --staged | --gate | --msg <file>
NAMING_PAT='c[l]aude'
NAMING_PAT2='noreply@anthropic\.com'

naming_self_test() {
	local probe; probe=$(printf 'c%saude' l)
	printf '%s\n' "$probe" | grep -qi -e "$NAMING_PAT" || { echo "naming: name pattern is broken" >&2; return 1; }
	local probe2; probe2=$(printf 'noreply@anthropi%s.com' c)
	printf '%s\n' "$probe2" | grep -qi -e "$NAMING_PAT2" || { echo "naming: address pattern is broken" >&2; return 1; }
	if printf '%s\n' "an ordinary line" | grep -qi -e "$NAMING_PAT"; then
		echo "naming: pattern matches everything" >&2; return 1
	fi
	return 0
}

# Input is passed as an argument, never through a pipeline, because a grep that
# finds nothing exits non-zero and that must not read as a violation.
naming_scan() { # label text
	local label="$1" text="$2" hits
	[ -z "$text" ] && return 0
	hits=$(printf '%s\n' "$text" | grep -n -i -e "$NAMING_PAT" -e "$NAMING_PAT2" || true)
	[ -z "$hits" ] && return 0
	printf 'REJECT %s: the tooling name or address appears here\n' "$label" >&2
	printf '%s\n' "$hits" | head -8 | sed 's/^/    /' >&2
	return 1
}

naming_scan_paths() { # paths
	local text="$1" hits
	[ -z "$text" ] && return 0
	hits=$(printf '%s\n' "$text" | grep -i -e "$NAMING_PAT" || true)
	[ -z "$hits" ] && return 0
	printf 'REJECT path name: a path carries the tooling name\n' >&2
	printf '%s\n' "$hits" | head -8 | sed 's/^/    /' >&2
	return 1
}

naming_staged() {
	local rc=0 paths added v
	paths=$(git diff --cached --name-only --diff-filter=ACMR || true)
	added=$(git diff --cached --diff-filter=ACMR -U0 | grep '^+' | grep -v '^+++' || true)
	naming_scan_paths "$paths" || rc=1
	naming_scan "staged content" "$added" || rc=1
	for v in "$(git config user.name || true)" "$(git config user.email || true)"; do
		if printf '%s\n' "$v" | grep -qi -e "$NAMING_PAT" -e "$NAMING_PAT2"; then
			echo "REJECT identity: user.name or user.email carries the tooling name" >&2; rc=1
		fi
	done
	return $rc
}

naming_tracked() {
	local rc=0 f hits=''
	naming_scan_paths "$(git ls-files || true)" || rc=1
	while IFS= read -r f; do
		[ -f "$f" ] || continue
		if grep -I -q -i -e "$NAMING_PAT" -e "$NAMING_PAT2" "$f" 2>/dev/null; then
			hits="$hits$f"$'\n'
		fi
	done < <(git ls-files)
	if [ -n "$hits" ]; then
		echo "REJECT tracked content: these tracked files carry the tooling name or address" >&2
		printf '%s' "$hits" | head -12 | sed 's/^/    /' >&2
		rc=1
	fi
	naming_scan_paths "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)" || rc=1
	return $rc
}

# Run the repository's own hook, if it ships one, with the same arguments and
# standard input. Used by the machine level dispatcher; harmless in a gate.
naming_delegate() {
	local hook="$1"; shift
	local root p rc
	root=$(git rev-parse --show-toplevel 2>/dev/null) || return 0
	for p in "scripts/hooks/$hook" ".githooks/$hook" "hooks/$hook"; do
		if [ -f "$root/$p" ]; then
			echo "naming: running the repository's own $p" >&2
			if [ -n "${NAMING_STDIN:-}" ] && [ -f "${NAMING_STDIN:-}" ]; then
				bash "$root/$p" "$@" < "$NAMING_STDIN"; rc=$?
			else
				bash "$root/$p" "$@"; rc=$?
			fi
			[ $rc -ne 0 ] && echo "repository hook $p refused (exit $rc)" >&2
			return $rc
		fi
	done
	return 0
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
	naming_self_test || exit 1
	case "${1:---gate}" in
		--self-test) exit 0 ;;
		--staged)    naming_staged; exit $? ;;
		--msg)       naming_scan "commit message" "$(cat "$2")"; exit $? ;;
		--gate)      naming_tracked; exit $? ;;
		*) echo "usage: naming.sh --self-test|--staged|--gate|--msg <file>" >&2; exit 2 ;;
	esac
fi
