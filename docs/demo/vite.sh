# Sourced by vite.tape: fetches vite at the commit bench/bench.toml pins into a temporary directory, without its history, and enters it.
pins="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/bench/bench.toml"
commit="$(grep -A 4 '^name = "vite"$' "$pins" | sed -n 's/^commit = "\([0-9a-f]*\)"$/\1/p')"
[ -n "$commit" ] || return 1
demo="$(mktemp -d)"
cd "$demo" || return 1
git init -q
git fetch -q --depth 1 https://github.com/vitejs/vite "$commit"
git -c advice.detachedHead=false checkout -q FETCH_HEAD
# The checkout wrote every file a moment ago, and gsttt, like git, takes a modification time as proof that a file is unchanged only once it is a few seconds old, so the first runs would hash every file again: wait until the tree looks like one somebody works in.
sleep 3
export PS1='$ '
