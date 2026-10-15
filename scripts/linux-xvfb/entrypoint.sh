#!/usr/bin/env bash
set -euo pipefail
source /usr/local/bin/session-env.sh
umask 077
[[ $(id -u) == 10001 && $(id -g) == 10001 ]] || {
    echo 'Refusing to run the desktop as any user other than 10001:10001.' >&2
    exit 1
}
geometry=${SCREEN_GEOMETRY:-1280x720x24}
[[ $geometry =~ ^[1-9][0-9]{2,3}x[1-9][0-9]{2,3}x24$ ]] || {
    echo 'SCREEN_GEOMETRY must be WIDTHxHEIGHTx24 (3-4 digit dimensions).' >&2
    exit 2
}
[[ ${ENABLE_FIXTURE:-0} == 0 || ${ENABLE_FIXTURE:-0} == 1 ]] || exit 2
mkdir -p "$XDG_RUNTIME_DIR"
chmod 0700 "$XDG_RUNTIME_DIR"
rm -f "$XDG_RUNTIME_DIR/ready" "$XDG_RUNTIME_DIR/services"
declare -a pids=() names=()

cleanup() {
    local status=$? pid deadline alive
    trap - EXIT TERM INT
    rm -f "$XDG_RUNTIME_DIR/ready"
    # Every service owns a separate process group, including any descendants.
    for pid in "${pids[@]}"; do kill -TERM -- "-$pid" 2>/dev/null || true; done
    deadline=$((SECONDS + 5))
    while (( SECONDS < deadline )); do
        alive=0
        for pid in "${pids[@]}"; do
            if kill -0 -- "-$pid" 2>/dev/null; then alive=1; fi
        done
        (( alive )) || break
        sleep 0.1
    done
    for pid in "${pids[@]}"; do kill -KILL -- "-$pid" 2>/dev/null || true; done
    for pid in "${pids[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -f "$XDG_RUNTIME_DIR/services" "$XAUTHORITY"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT

start_service() {
    local name=$1 pid
    shift
    setsid "$@" &
    pid=$!
    pids+=("$pid")
    names+=("$name")
    printf '%s %s\n' "$name" "$pid" >> "$XDG_RUNTIME_DIR/services"
    printf '[session] started %s pid=%s\n' "$name" "$pid"
}
check_services() {
    local i
    for i in "${!pids[@]}"; do
        if ! kill -0 "${pids[$i]}" 2>/dev/null; then
            echo "[session] critical service exited during startup: ${names[$i]}" >&2
            exit 1
        fi
    done
}
await_ready() {
    local description=$1 deadline=$((SECONDS + 30))
    shift
    while ! timeout 2s "$@" >/dev/null 2>&1; do
        check_services
        if (( SECONDS >= deadline )); then
            echo "[session] readiness timeout: $description" >&2
            exit 1
        fi
        sleep 0.2
    done
    check_services
}

# A new cookie on every start; never -ac or xhost +. No cookie in logs.
: > "$XAUTHORITY"
chmod 0600 "$XAUTHORITY"
cookie=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
xauth -f "$XAUTHORITY" add "$DISPLAY" MIT-MAGIC-COOKIE-1 "$cookie"
unset cookie
start_service xvfb Xvfb "$DISPLAY" -screen 0 "$geometry" -dpi 96 \
    -nolisten tcp -auth "$XAUTHORITY" +extension RANDR +extension XTEST -noreset
await_ready X11 xdpyinfo -display "$DISPLAY"
xdpyinfo -display "$DISPLAY" -queryExtensions > "$XDG_RUNTIME_DIR/xdpyinfo"
grep -q 'XTEST' "$XDG_RUNTIME_DIR/xdpyinfo"
grep -q 'RANDR' "$XDG_RUNTIME_DIR/xdpyinfo"
# display-info needs a real primary monitor. Use the actual server output name.
output=$(xrandr --query | awk '$2 == "connected" && !found {print $1; found=1}')
[[ -n $output ]] || { echo '[session] no connected RandR output' >&2; exit 1; }
xrandr --output "$output" --primary
printf 'Xft.dpi: 96\n' | xrdb -merge
xrandr --query > "$XDG_RUNTIME_DIR/xrandr"
xrandr --listactivemonitors > "$XDG_RUNTIME_DIR/monitors"
grep -Eq ' connected primary ' "$XDG_RUNTIME_DIR/xrandr"
grep -Eq '^[[:space:]]*[0-9]+: [^[:space:]]*\*' "$XDG_RUNTIME_DIR/monitors"
xrdb -query | grep -Eq '^Xft\.dpi:[[:space:]]+96$'
printf '[session] display=%s geometry=%s dpi=96 primary=%s\n' "$DISPLAY" "$geometry" "$output"

start_service openbox openbox --sm-disable
await_ready WM bash -c 'xprop -root _NET_SUPPORTING_WM_CHECK | grep -Eq "window id # 0x[1-9a-fA-F][0-9a-fA-F]*"'
# Disable blanking/screensaver; no host display or power settings are changed.
xset s off
xset s noblank
start_service x11vnc x11vnc -display "$DISPLAY" -auth "$XAUTHORITY" \
    -listen 127.0.0.1 -localhost -no6 -rfbport 5900 \
    -forever -shared -viewonly -nopw -noxdamage
await_ready VNC python3 -c 'import socket; s=socket.create_connection(("127.0.0.1",5900),1); assert s.recv(12).startswith(b"RFB "); s.close()'
start_service novnc websockify --web=/usr/share/novnc 0.0.0.0:6080 127.0.0.1:5900
await_ready noVNC python3 -c 'import urllib.request; o=urllib.request.build_opener(urllib.request.ProxyHandler({})); r=o.open("http://127.0.0.1:6080/vnc.html",timeout=1); assert r.status == 200; r.close()'
if [[ ${ENABLE_FIXTURE:-0} == 1 ]]; then
    start_service fixture python3 /opt/rpa-fixture/fixture.py
fi
# Optional command is supervised; docker exec commands started later are not.
if (( $# )); then start_service host "$@"; fi
check_services
touch "$XDG_RUNTIME_DIR/ready"
echo '[session] ready; observation is view-only, not a product TLS endpoint'
set +e
wait -n "${pids[@]}"
status=$?
set -e
echo "[session] critical service exited (status=$status); stopping all services" >&2
# Even a clean service exit is unexpected for this long-lived desktop.
exit 1
