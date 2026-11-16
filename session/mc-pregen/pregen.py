#!/usr/bin/env python3
"""Nightly Minecraft chunk pre-generation (azototv-mc-pregen.service, 01:00-07:00).

For every AzotoTV profile, the world it played last gets the area around its spawn
generated ahead of time (RADIUS blocks each way), so exploring there doesn't make
the game generate terrain while you play. A headless Fabric server (this folder:
fabric-server.jar, mods/ = links to the Controller instance's world-generation mods
+ Chunky) opens the world with the game clock frozen and runs Chunky; worlds that
are done are remembered in state.json. The service runs with a CPU limit at idle
priority so the fans stay quiet. AzotoTV stops the service before Minecraft starts
(SIGTERM: the server saves and quits; the next night starts over, skipping the
chunks that already exist).
"""
import gzip, json, os, re, signal, struct, subprocess, sys, tarfile, threading, time
from pathlib import Path

RADIUS = 1000
HOME = Path.home()
HERE = Path(__file__).resolve().parent
PLAYERS = HOME / ".local/share/azototv/players"
INSTANCE = HOME / ".local/share/PrismLauncher/instances/Controller"
JAVA = HOME / ".local/share/PrismLauncher/java/java-runtime-epsilon/bin/java"
STATE = HERE / "state.json"
BACKUP = HOME / "backup/mc-worlds"
# mods the server needs: the ones that shape the terrain, their dependencies, the
# world-generation optimisations the game itself runs, and Chunky
MODS = ["fabric-api-", "Chunky-Fabric-", "c2me-fabric-", "zfastnoise-", "zconfig-", "lithium-fabric-",
        "ScalableLux-", "Ksyxis-", "ferritecore-", "dungeons-and-taverns-", "SnowUnderTrees-", "mru-"]

stopping = False
server = None


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, flush=True)


def last_played(level_dat):
    """Data.LastPlayed from level.dat, and its offset in the uncompressed file."""
    raw = gzip.decompress(level_dat.read_bytes())
    pat = b"\x04" + struct.pack(">H", 10) + b"LastPlayed"
    i = raw.find(pat)
    return (struct.unpack_from(">q", raw, i + len(pat))[0], i + len(pat)) if i >= 0 else (0, -1)


def set_last_played(level_dat, value):
    """The server stamps LastPlayed when it saves: put the player's own time back,
    so AzotoTV's world list keeps showing when they really played."""
    raw = bytearray(gzip.decompress(level_dat.read_bytes()))
    _, off = last_played(level_dat)
    if off >= 0:
        raw[off:off + 8] = struct.pack(">q", value)
        level_dat.write_bytes(gzip.compress(bytes(raw)))


def worlds():
    """Each profile's last played world."""
    out = []
    for saves in sorted(PLAYERS.glob("*/minecraft/saves")):
        best = None
        for lvl in saves.glob("*/level.dat"):
            try:
                t, _ = last_played(lvl)
            except Exception as e:
                log("unreadable", lvl, e)
                continue
            if best is None or t > best[0]:
                best = (t, lvl.parent)
        if best:
            out.append(best[1])
    return out


def link_mods():
    mods = HERE / "mods"
    mods.mkdir(exist_ok=True)
    for f in mods.iterdir():
        f.unlink()
    have = sorted(p.name for p in (INSTANCE / "minecraft/mods").glob("*.jar"))
    for pre in MODS:
        jar = next((n for n in have if n.startswith(pre)), None)
        if not jar:
            sys.exit(f"mod missing from the instance: {pre}")
        (mods / jar).symlink_to(INSTANCE / "minecraft/mods" / jar)


def minecraft_running():
    return subprocess.run(["pgrep", "-x", "prismlauncher"], capture_output=True).returncode == 0


def backup_once(world):
    """A copy of each world before its first pre-generation."""
    BACKUP.mkdir(parents=True, exist_ok=True)
    name = re.sub(r"[^A-Za-z0-9_-]+", "_", f"{world.parents[2].name[:8]}-{world.name}")
    dest = BACKUP / f"{name}.tar.gz"
    if not dest.exists():
        with tarfile.open(dest, "w:gz") as t:
            t.add(world, arcname=world.name, filter=lambda i: None if i.name.endswith("session.lock") else i)
        log("backup", dest)


def generate(world):
    """True when Chunky finished the whole area."""
    global server
    for f in (HERE / "config/chunky/tasks").glob("**/*"):
        if f.is_file():
            f.unlink()
    lvl = world / "level.dat"
    played, _ = last_played(lvl)
    server = subprocess.Popen(
        [str(JAVA), "-Xms512m", "-Xmx2g", "-jar", "fabric-server.jar", "--nogui",
         "--universe", str(world.parent), "--world", world.name],
        cwd=HERE, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    ready, finished = threading.Event(), threading.Event()

    def reader():
        last = 0
        for line in server.stdout:
            if "Done (" in line:
                ready.set()
            if "Task finished" in line:
                finished.set()
            if "Task running" in line:
                # Chunky reports every second: one line every 5 minutes is enough
                if time.time() - last < 300:
                    continue
                last = time.time()
            elif "[Chunky]" not in line and "ERROR" not in line:
                continue
            log(line.rstrip()[:200])
    threading.Thread(target=reader, daemon=True).start()

    def send(cmd):
        try:
            server.stdin.write(cmd + "\n")
            server.stdin.flush()
        except (BrokenPipeError, ValueError):
            pass

    if ready.wait(600) and not stopping:
        # no time passes in the world (no day/night, crops, mobs) while it generates
        for c in ["tick freeze", "chunky world minecraft:overworld", "chunky spawn", f"chunky radius {RADIUS}", "chunky start"]:
            send(c)
            time.sleep(0.5)
        while not finished.is_set() and not stopping and server.poll() is None:
            finished.wait(2)
    send("stop")
    try:
        server.wait(180)
    except subprocess.TimeoutExpired:
        server.kill()
        server.wait()
    server = None
    set_last_played(lvl, played)
    return finished.is_set()


def on_term(*_):
    global stopping
    stopping = True
    log("stopping")


def main():
    signal.signal(signal.SIGTERM, on_term)
    signal.signal(signal.SIGINT, on_term)
    if minecraft_running():
        return log("Minecraft is open: nothing to do")
    state = json.loads(STATE.read_text()) if STATE.exists() else {}
    todo = [w for w in worlds() if state.get(str(w), {}).get("radius", 0) < RADIUS]
    if not todo:
        return log("every world is done")
    link_mods()
    for w in todo:
        if stopping:
            break
        log("world", w)
        backup_once(w)
        t0 = time.time()
        if generate(w):
            state[str(w)] = {"radius": RADIUS, "done": time.strftime("%Y-%m-%d %H:%M")}
            STATE.write_text(json.dumps(state, indent=1))
            log(f"done in {(time.time() - t0) / 60:.0f} min")


main()
