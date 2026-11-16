#!/usr/bin/env python3
# AzotoTV: the yt-dlp that mpv's ytdl hook runs (see ytdl.rs). YouTube hands
# out stream links that only work from a given moment (`available_at`, a few
# seconds later): yt-dlp waits before downloading itself, but mpv opens them at
# once and gets 403 Forbidden. This runs the real yt-dlp ($AZOTOTV_YTDLP) and
# returns its answer once the links work.
import json
import os
import subprocess
import sys
import time

real = os.environ.get("AZOTOTV_YTDLP") or "yt-dlp"
r = subprocess.run([real, *sys.argv[1:]], stdout=subprocess.PIPE)
out = r.stdout


def times(d):
    if isinstance(d, dict):
        if isinstance(d.get("available_at"), (int, float)):
            yield d["available_at"]
        for k in ("requested_formats", "entries"):
            for x in d.get(k) or []:
                yield from times(x)


try:
    info = json.loads(out)
except ValueError:
    info = None  # not the JSON answer (--version, an error): as it is
if info is not None:
    wait = max(times(info), default=0) - time.time()
    if wait > 0:
        time.sleep(min(wait + 0.3, 30))
sys.stdout.buffer.write(out)
sys.exit(r.returncode)
