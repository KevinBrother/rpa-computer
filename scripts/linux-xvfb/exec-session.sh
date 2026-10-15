#!/usr/bin/env bash
set -euo pipefail
source /usr/local/bin/session-env.sh
if [[ $(id -u) != 10001 || $(id -g) != 10001 ]]; then
    echo 'Run host/fixture as container user 10001:10001, not root.' >&2
    exit 1
fi
if [[ ! -f $XDG_RUNTIME_DIR/ready || ! -r $XAUTHORITY ]]; then
    echo 'X11 session is not ready; inspect container health/logs first.' >&2
    exit 1
fi
if (( $# == 0 )); then
    echo 'Usage: exec-session.sh COMMAND [ARG ...]' >&2
    exit 2
fi
exec "$@"
