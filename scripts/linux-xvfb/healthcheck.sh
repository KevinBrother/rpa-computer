#!/usr/bin/env bash
set -euo pipefail
source /usr/local/bin/session-env.sh
[[ $(id -u) == 10001 && -f $XDG_RUNTIME_DIR/ready ]]
while read -r name pid; do
    [[ -n $name && $pid =~ ^[0-9]+$ ]]
    kill -0 "$pid"
done < "$XDG_RUNTIME_DIR/services"
info=$(timeout 2s xdpyinfo -display "$DISPLAY" -queryExtensions)
[[ $info == *XTEST* && $info == *RANDR* ]]
size=${SCREEN_GEOMETRY:-1280x720x24}
size=${size%x24}
grep -Eq "dimensions:[[:space:]]+${size}[[:space:]]+pixels" <<< "$info"
grep -Eq 'resolution:[[:space:]]+96x96 dots per inch' <<< "$info"
xrdb -query | grep -Eq '^Xft\.dpi:[[:space:]]+96$'
xrandr --query | grep -Eq ' connected primary '
xrandr --listactivemonitors | grep -Eq '^[[:space:]]*[0-9]+: [^[:space:]]*\*'
wm=$(xprop -root _NET_SUPPORTING_WM_CHECK)
[[ $wm =~ 0x[1-9a-fA-F][0-9a-fA-F]* ]]
xprop -id "${BASH_REMATCH[0]}" _NET_WM_NAME | grep -q Openbox
python3 - <<'PY'
import socket
import urllib.request
with socket.create_connection(("127.0.0.1", 5900), timeout=1) as connection:
    assert connection.recv(12).startswith(b"RFB "), "VNC greeting missing"
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
with opener.open("http://127.0.0.1:6080/vnc.html", timeout=1) as response:
    assert response.status == 200, "noVNC HTTP failed"
PY
