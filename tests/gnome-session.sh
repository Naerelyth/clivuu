#!/bin/sh
set -eu
ulimit -c 0
observer_uuid=${CLIVUU_TEST_EXTENSION_UUID:?Test observer UUID is required}
if [ "${CLIVUU_TEST_PANEL:-}" = 1 ]; then
    gsettings --schemadir "$XDG_DATA_HOME/gnome-shell/extensions/dash-to-panel@jderose9.github.com/schemas" set org.gnome.shell.extensions.dash-to-panel intellihide true
    gsettings --schemadir "$XDG_DATA_HOME/gnome-shell/extensions/dash-to-panel@jderose9.github.com/schemas" set org.gnome.shell.extensions.dash-to-panel intellihide-hide-from-windows true
    gsettings set org.gnome.shell enabled-extensions "['dash-to-panel@jderose9.github.com', '$observer_uuid']"
else
    gsettings set org.gnome.shell enabled-extensions "['$observer_uuid']"
fi
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '50'
gsettings set org.gnome.desktop.interface enable-hot-corners false
if [ "${CLIVUU_TEST_MIXED_MONITORS:-0}" = 1 ]; then
    gsettings set org.gnome.mutter experimental-features "['scale-monitor-framebuffer']"
fi
shell_pid= pipewire_pid= wireplumber_pid=
cleanup() {
    for pid in "$shell_pid" "$wireplumber_pid" "$pipewire_pid"; do
        if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; fi
    done
    wait 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
# Do not inherit host daemon addresses/configuration. Client connections remain
# on the private PipeWire socket in this test's XDG_RUNTIME_DIR.
unset PIPEWIRE_REMOTE PIPEWIRE_RUNTIME_DIR PIPEWIRE_CORE PULSE_SERVER PULSE_RUNTIME_PATH
unset PIPEWIRE_CONFIG_DIR PIPEWIRE_CONFIG_NAME PIPEWIRE_CONFIG_PREFIX WIREPLUMBER_CONFIG_DIR
export PIPEWIRE_CONFIG_DIR="$CLIVUU_TEST_DIR/media/pipewire"
PIPEWIRE_CONFIG_NAME=pipewire.conf \
    pipewire > "$CLIVUU_TEST_DIR/pipewire.log" 2>&1 &
pipewire_pid=$!
DBUS_SYSTEM_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/no-system-bus" \
    WIREPLUMBER_CONFIG_DIR="$CLIVUU_TEST_DIR/media/wireplumber" \
    wireplumber --profile clivuu-test > "$CLIVUU_TEST_DIR/wireplumber.log" 2>&1 &
wireplumber_pid=$!
python3 -B "$(dirname "$0")/media-isolation.py" "$CLIVUU_TEST_DIR" "$wireplumber_pid" "$pipewire_pid"
set -- "--virtual-monitor=${CLIVUU_TEST_MONITOR_SIZE:-1280x800}"
if [ "${CLIVUU_TEST_TWO_MONITORS:-0}" = 1 ]; then
    set -- "$@" "--virtual-monitor=${CLIVUU_TEST_SECOND_MONITOR_SIZE:-1024x768}"
fi
gnome-shell --headless --wayland --wayland-display=wayland-clivuu-test "$@" --debug-control > "$CLIVUU_TEST_DIR/shell.log" 2>&1 &
shell_pid=$!
attempt=0
while [ ! -f "$CLIVUU_TEST_DIR/results.json" ]; do
    if ! kill -0 "$shell_pid" 2>/dev/null || ! kill -0 "$pipewire_pid" 2>/dev/null || ! kill -0 "$wireplumber_pid" 2>/dev/null; then
        cat "$CLIVUU_TEST_DIR/shell.log"
        exit 1
    fi
    attempt=$((attempt + 1))
    if [ "$attempt" -gt 300 ]; then
        tail -60 "$CLIVUU_TEST_DIR/shell.log"
        exit 1
    fi
    sleep 1
done
python3 -B "$(dirname "$0")/media-isolation.py" "$CLIVUU_TEST_DIR" "$wireplumber_pid" "$pipewire_pid"
cat "$CLIVUU_TEST_DIR/results.json"
