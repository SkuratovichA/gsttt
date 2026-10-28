#!/bin/sh
# Runs gsttt for the plugin. When gsttt is not installed, a session start says how to install it and every other event does nothing, so that a missing binary never breaks a session.

if command -v gsttt > /dev/null 2>&1; then
  exec gsttt "$@"
fi

if [ "${1:-}" = hook ] && [ "${2:-}" = session-start ]; then
  echo 'gsttt is not installed. Install it with: curl -fsSL https://raw.githubusercontent.com/SkuratovichA/gsttt/main/install.sh | sh'
fi
exit 0
