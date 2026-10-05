#!/usr/bin/env bash
# Dash rule check, shared by this repository's gate so the rule is enforced by
# machine rather than remembered.
#
# Three classes are counted, named by code point so this file never contains the
# characters it looks for and therefore never matches itself:
#
#   U+2013 en dash        UTF-8 e2 80 93
#   U+2212 minus sign     UTF-8 e2 88 92
#   U+2014 em dash        UTF-8 e2 80 94
#
# U+2212 is in scope by the decision recorded under AUD-051. A reader cannot
# tell a minus sign from an en dash on screen, so the rule is no dash-like
# character rather than no em or en dash. The target is zero, from a measured
# baseline of 58 across 176 readable tracked text files.
#
# Each class carries its expected UTF-8 encoding and self_test checks the built
# pattern against it. That is the point of the third column rather than
# decoration: the first version of this file assembled each pattern as a
# backslash-u escape around a variable. Measured in this bash, that form yields
# the six ASCII bytes of the escape rather than the character, while the same
# escape written as a literal expands. The check therefore searched for a string
# no file contains and passed on a tree holding 58 violations, and its self test
# passed alongside it because it planted the identical wrong string. A self test
# that plants what the detector looks for cannot see the two being wrong
# together, so the patterns are tied to the encoding instead.
#
# Usable two ways:
#   source it, then call dashes_tracked
#   run it:  dashes.sh --self-test | --gate

# code point, escape form, expected UTF-8 bytes
DASH_SPEC='2013:\xe2\x80\x93:e28093 2212:\xe2\x88\x92:e28892 2014:\xe2\x80\x94:e28094'
DASH_BASELINE=58

dashes_args=()
dashes_build() {
	dashes_args=()
	local spec esc
	for spec in $DASH_SPEC; do
		esc=${spec#*:}
		esc=${esc%:*}
		dashes_args+=( -e "$(printf '%b' "$esc")" )
	done
}

dashes_hex() { # string
	printf '%s' "$1" | od -An -tx1 | tr -d ' \n'
}

dashes_self_test() {
	dashes_build
	if [ "${#dashes_args[@]}" -ne 6 ]; then
		echo "dashes: expected three patterns, built ${#dashes_args[@]} arguments" >&2
		return 1
	fi

	local tmp spec cp want built hex probe i=1
	tmp=$(mktemp -d) || return 1
	# shellcheck disable=SC2064  # expand tmp now so the trap cannot lose it
	trap "rm -rf '$tmp'" RETURN

	for spec in $DASH_SPEC; do
		cp=${spec%%:*}
		want=${spec##*:}
		built=${dashes_args[$i]}
		i=$((i + 2))

		# The pattern must be the character, not an escape that failed to
		# expand. Checked against the code point's UTF-8 encoding, which is a
		# fact outside this file.
		hex=$(dashes_hex "$built")
		if [ "$hex" != "$want" ]; then
			echo "dashes: U+$cp built as bytes $hex, expected $want" >&2
			return 1
		fi
		case "$built" in
			*'\'*|*u*)
				echo "dashes: U+$cp pattern still carries an unexpanded escape" >&2
				return 1 ;;
		esac

		probe="$tmp/probe_$cp.txt"
		printf 'a%sb\n' "$built" > "$probe"
		if ! grep -q "${dashes_args[@]}" "$probe"; then
			echo "dashes: U+$cp is in the list but the check does not find it" >&2
			return 1
		fi
		if [ "$(dashes_count_file "$probe")" != "1" ]; then
			echo "dashes: counted $(dashes_count_file "$probe") of 1 planted U+$cp" >&2
			return 1
		fi
	done

	printf 'an ordinary line with a hyphen-minus and no dash\n' > "$tmp/clean.txt"
	if grep -q "${dashes_args[@]}" "$tmp/clean.txt"; then
		echo "dashes: the check matches a line that carries none" >&2
		return 1
	fi
	if [ "$(dashes_count_file "$tmp/clean.txt")" != "0" ]; then
		echo "dashes: a clean file counted non-zero" >&2
		return 1
	fi

	# The three outcomes of the gate itself, over a planted list rather than the
	# working repository, because a control never writes there. Without these the
	# step passes on a swept tree and nothing shows it can still fail, which is
	# the state the first version of this file shipped in.
	local rc
	printf '%s\n' "$tmp/clean.txt" > "$tmp/list_clean"
	printf '%s\n' "$tmp/clean.txt" "$tmp/probe_2013.txt" > "$tmp/list_dirty"
	: > "$tmp/list_empty"

	DASH_FILE_LIST="$tmp/list_clean" dashes_tracked > /dev/null 2>&1
	rc=$?
	if [ "$rc" -ne 0 ]; then
		echo "dashes: a clean list was rejected (exit $rc)" >&2
		return 1
	fi

	DASH_FILE_LIST="$tmp/list_dirty" dashes_tracked > /dev/null 2>&1
	rc=$?
	if [ "$rc" -eq 0 ]; then
		echo "dashes: a planted dash was not rejected, so the gate cannot fail" >&2
		return 1
	fi

	DASH_FILE_LIST="$tmp/list_empty" dashes_tracked > /dev/null 2>&1
	rc=$?
	if [ "$rc" -eq 0 ]; then
		echo "dashes: an empty list passed, so a count of zero proves nothing" >&2
		return 1
	fi
	return 0
}

# grep exits non-zero when it finds nothing, which must read as zero rather than
# as a failure, so the count is taken with the exit status discarded.
dashes_count_file() { # path
	local n
	n=$(grep -o "${dashes_args[@]}" "$1" 2>/dev/null | wc -l | tr -d ' ')
	printf '%s' "${n:-0}"
}

# The list of files to scan. `git ls-files` in a real run, and a named file when
# the self test needs to scan a planted tree. The indirection exists so the
# failure path can be controlled without planting anything in the working
# repository, which is where a control must never write.
dashes_file_list() {
	if [ -n "${DASH_FILE_LIST:-}" ]; then
		cat "$DASH_FILE_LIST"
	else
		git ls-files
	fi
}

dashes_tracked() {
	dashes_build
	local f total=0 n text=0 skipped=0 report=''
	while IFS= read -r f; do
		[ -f "$f" ] || continue
		# -I makes grep treat a binary file as a non-match. Counting those as
		# skipped rather than clean keeps the floor below honest.
		if ! grep -Iq '' "$f" 2>/dev/null; then
			skipped=$((skipped + 1))
			continue
		fi
		text=$((text + 1))
		n=$(dashes_count_file "$f")
		if [ "$n" -gt 0 ]; then
			total=$((total + n))
			report="$report$(printf '    %-58s %s' "$f" "$n")"$'\n'
		fi
	done < <(dashes_file_list)

	if [ "$text" -eq 0 ]; then
		echo "dashes: read no tracked text file, so a count of zero proves nothing" >&2
		return 1
	fi

	printf 'dashes: counted U+2013, U+2212 and U+2014 in %d tracked text files, %d skipped as binary\n' \
		"$text" "$skipped"
	if [ "$total" -eq 0 ]; then
		printf 'dashes: none found, which is the rule. Baseline was %d on 2026-10-04.\n' \
			"$DASH_BASELINE"
		return 0
	fi
	printf 'REJECT dashes: %d dash-like characters in tracked content, target 0 (baseline %d)\n' \
		"$total" "$DASH_BASELINE" >&2
	printf '%s' "$report" | head -24 >&2
	printf '    U+2212 is in scope by the AUD-051 decision; a hyphen-minus is what belongs here\n' >&2
	return 1
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
	dashes_self_test || exit 1
	case "${1:---gate}" in
		--self-test) exit 0 ;;
		--gate)      dashes_tracked; exit $? ;;
		*) echo "usage: dashes.sh --self-test|--gate" >&2; exit 2 ;;
	esac
fi
