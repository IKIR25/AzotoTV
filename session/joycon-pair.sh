#!/bin/sh
# Pairs Nintendo controllers (Joy-Cons, Pro Controller) over Bluetooth and
# marks them trusted, so afterwards a button press is enough to reconnect.
# Hold each controller's small sync button (LEDs running) before starting.
# Usage: joycon-pair.sh [seconds to scan, default 30]
SECS=${1:-30}

bluetoothctl power on >/dev/null
echo "Scanning ${SECS}s: hold the sync button on every controller..."
bluetoothctl --timeout "$SECS" scan on >/dev/null &
scan=$!
done_macs=""
end=$(( $(date +%s) + SECS ))
while [ "$(date +%s)" -lt "$end" ]; do
  bluetoothctl devices | grep -E "Joy-Con|Pro Controller" | while read -r _ mac name; do
    case " $(cat "${XDG_RUNTIME_DIR:-/tmp}/joycon-pair.done" 2>/dev/null) " in *" $mac "*) continue ;; esac
    if bluetoothctl info "$mac" | grep -q "Paired: yes"; then
      echo "$name ($mac): already paired"
    else
      echo "$name ($mac): pairing..."
      bluetoothctl pair "$mac" >/dev/null 2>&1 || { echo "  pair failed, retrying next pass"; continue; }
    fi
    bluetoothctl trust "$mac" >/dev/null
    bluetoothctl connect "$mac" >/dev/null 2>&1
    sleep 1
    if bluetoothctl info "$mac" | grep -q "Connected: yes"; then
      echo "  connected + trusted"
      echo "$mac" >> "${XDG_RUNTIME_DIR:-/tmp}/joycon-pair.done"
    else
      echo "  not connected yet, retrying next pass"
    fi
  done
  sleep 2
done
wait "$scan" 2>/dev/null
rm -f "${XDG_RUNTIME_DIR:-/tmp}/joycon-pair.done"
echo "Paired Nintendo controllers:"
bluetoothctl devices Paired | grep -E "Joy-Con|Pro Controller" || echo "  none"
echo "Now press L+R (one on each Joy-Con) for a single pad, or SL+SR on one Joy-Con to use it alone."
