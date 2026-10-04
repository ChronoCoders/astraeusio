#!/usr/bin/env bash
# The Rust version is written in one place. This refuses any second copy of it.
#
# backend/rust-toolchain.toml is that place. backend/Dockerfile takes the version
# from it by copying the file into the builder and letting rustup honour it, so
# the base image tag fixes only the Debian release. Before that the gate ran
# 1.93 while production built on rust:1.88, five minor versions apart, and
# nothing in the repository could see the gap.
#
# The rule is about any version, not the current one. A check that only knew
# today's number would pass the moment someone reached for a different one,
# which is the whole failure being prevented.
#
# Comments are stripped before scanning. The Dockerfile's own comment records
# the 1.93 and 1.88 history, and that is worth keeping; a directive naming a
# version is not. Scanning the raw file would fail on its own explanation.
#
# Usable two ways:
#   source it, then call toolchain_gate
#   run it:  toolchain.sh --self-test | --gate

TOOLCHAIN_FILE='backend/rust-toolchain.toml'
TOOLCHAIN_DOCKERFILE='backend/Dockerfile'

# A dotted version anywhere in an image tag, a rustup command, a cargo toolchain
# override, or a build argument. `rust:1-bookworm` is deliberately allowed: it
# names a major that every Rust release shares and decides nothing, because
# rustup overrides it from the toolchain file.
TOOLCHAIN_PATS=(
	'rust:[0-9]+\.[0-9]+'
	'rustup[[:space:]]+(install|default|toolchain[[:space:]]+install|override)[[:space:]]+[0-9]+\.[0-9]+'
	'cargo[[:space:]]+\+[0-9]+\.[0-9]+'
	'(ARG|ENV)[[:space:]]+[A-Z_]*RUST[A-Z_]*(VERSION|TOOLCHAIN|CHANNEL)[A-Z_]*=?[[:space:]]*"?[0-9]+\.[0-9]+'
	'RUST_(VERSION|TOOLCHAIN|CHANNEL)=[\"]?[0-9]+\.[0-9]+'
)

# Directive text only: everything from an unquoted # to end of line is dropped.
toolchain_directives() { # file
	sed 's/#.*$//' "$1"
}

toolchain_self_test() {
	local tmp rc=0
	tmp=$(mktemp -d) || return 1

	# Each forbidden form must be detected. A detector that matched nothing
	# would report a clean Dockerfile for any content at all.
	local -a bad=(
		'FROM rust:1.88-bookworm AS builder'
		'FROM rust:1.93.1-bookworm AS builder'
		'FROM docker.io/library/rust:2.0-trixie'
		'RUN rustup install 1.93.1'
		'RUN rustup default 1.94'
		'RUN rustup toolchain install 1.90.0'
		'RUN cargo +1.93.1 build --release'
		'ARG RUST_VERSION=1.93.1'
		'ENV RUST_TOOLCHAIN="1.94.0"'
	)
	local line
	for line in "${bad[@]}"; do
		printf '%s\n' "$line" > "$tmp/Dockerfile"
		if toolchain_scan "$tmp/Dockerfile" >/dev/null 2>&1; then
			echo "toolchain: a pinned version was not detected: $line" >&2
			rc=1
		fi
	done

	# And the permitted forms must pass, or the check would block every build.
	local -a good=(
		'FROM rust:1-bookworm AS builder'
		'COPY rust-toolchain.toml ./'
		'RUN rustup show'
		'RUN cargo build --release'
		'# this built 1.88 while the gate ran 1.93, which is the comment case'
		'FROM debian:bookworm-slim'
	)
	for line in "${good[@]}"; do
		printf '%s\n' "$line" > "$tmp/Dockerfile"
		if ! toolchain_scan "$tmp/Dockerfile" >/dev/null 2>&1; then
			echo "toolchain: a permitted line was rejected: $line" >&2
			rc=1
		fi
	done

	# A comment and a directive on the same line: the directive still counts.
	printf '%s\n' 'FROM rust:1.93-bookworm # pinned' > "$tmp/Dockerfile"
	if toolchain_scan "$tmp/Dockerfile" >/dev/null 2>&1; then
		echo "toolchain: a version before a trailing comment was missed" >&2
		rc=1
	fi

	rm -rf "$tmp"
	return $rc
}

toolchain_scan() { # dockerfile
	local f="$1" pat hits all=''
	if [ ! -f "$f" ]; then
		echo "toolchain: $f does not exist, so this check proves nothing" >&2
		return 1
	fi
	for pat in "${TOOLCHAIN_PATS[@]}"; do
		hits=$(toolchain_directives "$f" | grep -n -E -i -e "$pat" || true)
		[ -n "$hits" ] && all="$all$hits"$'\n'
	done
	[ -z "$all" ] && return 0
	printf 'REJECT %s: a Rust version is named here\n' "$f" >&2
	printf '%s' "$all" | head -8 | sed 's/^/    /' >&2
	printf '  the version belongs in %s and nowhere else\n' "$TOOLCHAIN_FILE" >&2
	return 1
}

toolchain_gate() {
	local root rc=0 chan
	root=$(git rev-parse --show-toplevel 2>/dev/null) || {
		echo "toolchain: not in a git repository" >&2; return 1; }

	if [ ! -f "$root/$TOOLCHAIN_FILE" ]; then
		echo "REJECT $TOOLCHAIN_FILE is missing: the toolchain is unpinned" >&2
		return 1
	fi

	# A floor. A toolchain file with no channel pins nothing, and the Dockerfile
	# scan below would then be guarding an empty promise.
	chan=$(grep -E '^[[:space:]]*channel[[:space:]]*=' "$root/$TOOLCHAIN_FILE" || true)
	if [ -z "$chan" ]; then
		echo "REJECT $TOOLCHAIN_FILE names no channel, so nothing is pinned" >&2
		rc=1
	elif printf '%s\n' "$chan" | grep -qE '"(stable|beta|nightly)"'; then
		echo "REJECT $TOOLCHAIN_FILE tracks a channel rather than a version: $chan" >&2
		echo "  a channel means a new lint fails a build that changed nothing" >&2
		rc=1
	fi

	# The Dockerfile must also actually take the version from that file, or the
	# absence of a number there means it is using whatever the base ships.
	if ! grep -qE '^[[:space:]]*COPY[[:space:]]+.*rust-toolchain\.toml' "$root/$TOOLCHAIN_DOCKERFILE"; then
		echo "REJECT $TOOLCHAIN_DOCKERFILE never copies rust-toolchain.toml" >&2
		echo "  without it the build compiles with whatever the base image ships" >&2
		rc=1
	fi

	toolchain_scan "$root/$TOOLCHAIN_DOCKERFILE" || rc=1
	return $rc
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
	toolchain_self_test || exit 1
	case "${1:---gate}" in
		--self-test) exit 0 ;;
		--gate)      toolchain_gate; exit $? ;;
		*) echo "usage: toolchain.sh --self-test|--gate" >&2; exit 2 ;;
	esac
fi
