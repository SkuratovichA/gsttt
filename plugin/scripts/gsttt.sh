#!/bin/sh
# Runs gsttt for the plugin, so that a missing or broken binary never breaks a session.
#
# A hook ends with status 0 and nothing on stderr whatever gsttt does, because Claude Code shows a failing hook to the user as an error. The server keeps the status and the complaints of gsttt.
#
# gsttt is looked for on PATH, then where install.sh puts it: Claude Code started from a launcher may not have ~/.local/bin on its PATH. When it is nowhere, a session start prints one note for the user, every other hook event does nothing, and the server stops with one line on stderr.

install_command='curl -fsSL https://raw.githubusercontent.com/SkuratovichA/gsttt/main/install.sh | sh'
install_dir="${GSTTT_INSTALL_DIR:-$HOME/.local/bin}"

if command -v gsttt > /dev/null 2>&1; then
  binary=gsttt
elif [ -x "$install_dir/gsttt" ]; then
  binary="$install_dir/gsttt"
else
  if [ "${1:-}" = hook ]; then
    # What a hook prints goes into the context of the model, so this is written for the user and gives the model nothing to run.
    if [ "${2:-}" = session-start ]; then
      echo "Note for the user: gsttt is not installed, so its tools are not available in this session. It can be installed with: $install_command"
    fi
    exit 0
  fi
  echo "gsttt is not installed, so the gsttt server cannot start. Install it with: $install_command" >&2
  exit 1
fi

if [ "${1:-}" = hook ]; then
  "$binary" "$@" 2> /dev/null || true
  exit 0
fi
exec "$binary" "$@"
