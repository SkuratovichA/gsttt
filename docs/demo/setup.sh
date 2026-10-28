# Sourced by the tapes in this directory: copies the example named by $1 into a new git repository in a temporary directory, commits it with a fixed author and date so that its commit id never changes, and enters it.
example="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$1"
demo="$(mktemp -d)"
cp -R "$example/." "$demo"
cd "$demo" || return 1
git init -q -b main
git add -A
GIT_AUTHOR_DATE=2026-10-31T12:00:00Z GIT_COMMITTER_DATE=2026-10-31T12:00:00Z git -c user.name=gsttt -c user.email=demo@gsttt.invalid -c commit.gpgsign=false -c core.hooksPath=/dev/null commit -q -m 'A web client and the API it calls'
export PS1='$ '
