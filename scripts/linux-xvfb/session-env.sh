# Shared by the supervisor, healthcheck and docker-exec wrapper.
# Do not inherit WSLg / Wayland / host X11 session values.
export HOME=/home/rpa USER=rpa DISPLAY=:99 XDG_SESSION_TYPE=x11
export XDG_RUNTIME_DIR=/home/rpa/.runtime XAUTHORITY=/home/rpa/.Xauthority
unset WAYLAND_DISPLAY WAYLAND_SOCKET
