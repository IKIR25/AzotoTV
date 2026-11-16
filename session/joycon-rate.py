#!/usr/bin/env python3
"""Measures how steadily Nintendo controllers report over Bluetooth.

The IMU device of a Joy-Con / Pro Controller (hid-nintendo) sends a report
all the time, even when nothing is pressed, so its timing shows the link
quality: a healthy link is ~66 reports/s (every ~15 ms) with no long gaps.
Gaps over 50 ms are the stutters you feel as lag; a gap that never ends is a
disconnect. Each report carries 3 motion samples, so events come in bursts:
a burst is one report.

Usage: joycon-rate.py [seconds, default 30]
"""
import select
import sys
import time

import evdev

secs = float(sys.argv[1]) if len(sys.argv) > 1 else 30
devs = [evdev.InputDevice(p) for p in evdev.list_devices()]
imus = [d for d in devs if d.info.vendor == 0x057E and "IMU" in d.name]
if not imus:
    sys.exit("no Joy-Con/Pro Controller IMU device: are they connected?")

stats = {d.fd: {"name": d.name, "last": None, "reports": 0, "gaps": [], "maxgap": 0.0} for d in imus}
by_fd = {d.fd: d for d in imus}
print(f"measuring {secs:.0f}s:", ", ".join(d.name for d in imus))
end = time.monotonic() + secs
while time.monotonic() < end:
    r, _, _ = select.select(list(by_fd), [], [], 0.5)
    for fd in r:
        s = stats[fd]
        try:
            events = list(by_fd[fd].read())
        except OSError:
            print(f"{s['name']}: DISCONNECTED")
            del by_fd[fd]
            continue
        for ev in events:
            if ev.type != evdev.ecodes.EV_SYN:
                continue
            t = ev.timestamp()
            # samples of the same report arrive within a couple of ms
            if s["last"] is not None and t - s["last"] < 0.004:
                s["last"] = t
                continue
            if s["last"] is not None:
                gap = t - s["last"]
                s["maxgap"] = max(s["maxgap"], gap)
                if gap > 0.050:
                    s["gaps"].append(gap)
            s["reports"] += 1
            s["last"] = t
    if not by_fd:
        break

for s in stats.values():
    long_gaps = s["gaps"]
    print(
        f"{s['name']}: {s['reports'] / secs:.1f} reports/s, max gap {s['maxgap'] * 1000:.0f} ms, "
        f"{len(long_gaps)} gaps > 50 ms"
        + (f" (worst {max(long_gaps) * 1000:.0f} ms)" if long_gaps else "")
    )
