#!/bin/sh
# Invoked inside a private mount, PID, IPC and network namespace. The reference
# app cannot contact the user's running Snipaste instance or its X server.
set -eu
ulimit -c 0
export LD_PRELOAD="$CLIVUU_TEST_DIR/reference-hook.so"
dbus-monitor --session "type='method_call',interface='org.gnome.Shell.Screenshot'" "type='method_call',interface='org.freedesktop.portal.Screenshot'" > "$CLIVUU_TEST_DIR/reference-capture-dbus.log" 2>&1 &
monitor_pid=$!
"$CLIVUU_REFERENCE_APP/AppRun" --config="$CLIVUU_TEST_DIR/snipaste.ini" > "$CLIVUU_TEST_DIR/snipaste.log" 2>&1 &
reference_pid=$!
trap 'kill "$reference_pid" "$monitor_pid" 2>/dev/null || true' EXIT
sleep 5
echo "primary $reference_pid" >> "$CLIVUU_TEST_DIR/snipaste.log"
strace -f -e trace=connect,write -s 512 -o "$CLIVUU_TEST_DIR/ipc.trace" timeout 10 "$CLIVUU_REFERENCE_APP/AppRun" paste --files "$CLIVUU_TEST_DIR/palette.png" --pos 350 250 >> "$CLIVUU_TEST_DIR/snipaste.log" 2>&1 || true
echo "paste dispatched" >> "$CLIVUU_TEST_DIR/snipaste.log"
"$CLIVUU_REFERENCE_APP/AppRun" paste --plain 'REFERENCE_CLIVUU_123' >> "$CLIVUU_TEST_DIR/snipaste.log" 2>&1 || true
"$CLIVUU_REFERENCE_APP/AppRun" show-images >> "$CLIVUU_TEST_DIR/snipaste.log" 2>&1 || true
touch "$CLIVUU_TEST_DIR/reference-ready"
attempt=0
snip_dispatched=0
while [ ! -f "$CLIVUU_TEST_DIR/reference-done" ]; do
    attempt=$((attempt + 1))
    [ "$attempt" -lt 60 ] || exit 1
    if [ "$snip_dispatched" = 0 ] && [ -f "$CLIVUU_TEST_DIR/reference-do-snip" ]; then
        "$CLIVUU_REFERENCE_APP/AppRun" snip --full >> "$CLIVUU_TEST_DIR/snipaste.log" 2>&1 || true
        snip_dispatched=1
        touch "$CLIVUU_TEST_DIR/reference-snip-dispatched"
    fi
    sleep 1
done
