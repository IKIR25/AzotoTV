// azoto-vscreen: a virtual screen in KWin (Plasma 6, Wayland) for game streaming.
//
// A 16:10 (or any) PC monitor streamed to a 16:9 TV gets black bars. KWin can
// make a virtual output of any size (the same thing KDE's "virtual monitor"
// uses): it lives as long as the screencast stream that asked for it.
//
//   azoto-vscreen start [WIDTHxHEIGHT] [NAME]   returns once the screen exists
//   azoto-vscreen stop  [NAME]                  removes it
//   azoto-vscreen run   [WIDTHxHEIGHT] [NAME]   same as start, in the foreground
//
// KWin only gives zkde_screencast_unstable_v1 to clients that have a desktop
// file listing it (X-KDE-Wayland-Interfaces) whose Exec is this binary.
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <wayland-client.h>

#include "zkde-screencast-unstable-v1-client-protocol.h"

static struct zkde_screencast_unstable_v1 *screencast;
static uint32_t screencast_version;
static int state; // 0 waiting, 1 created, -1 failed/closed
static volatile sig_atomic_t quit;

static void global(void *data, struct wl_registry *reg, uint32_t name, const char *iface, uint32_t version) {
    (void)data;
    if (strcmp(iface, zkde_screencast_unstable_v1_interface.name) == 0) {
        screencast_version = version < 4 ? version : 4;
        screencast = wl_registry_bind(reg, name, &zkde_screencast_unstable_v1_interface, screencast_version);
    }
}
static void global_remove(void *data, struct wl_registry *reg, uint32_t name) { (void)data; (void)reg; (void)name; }
static const struct wl_registry_listener registry_listener = { global, global_remove };

static void on_closed(void *data, struct zkde_screencast_stream_unstable_v1 *s) {
    (void)data; (void)s;
    fprintf(stderr, "azoto-vscreen: KWin closed the screen\n");
    state = -1;
}
static void on_created(void *data, struct zkde_screencast_stream_unstable_v1 *s, uint32_t node) {
    (void)data; (void)s;
    fprintf(stderr, "azoto-vscreen: screen created (pipewire node %u)\n", node);
    state = 1;
}
static void on_failed(void *data, struct zkde_screencast_stream_unstable_v1 *s, const char *error) {
    (void)data; (void)s;
    fprintf(stderr, "azoto-vscreen: KWin refused: %s\n", error);
    state = -1;
}
static const struct zkde_screencast_stream_unstable_v1_listener stream_listener = { on_closed, on_created, on_failed };

static void on_signal(int sig) { (void)sig; quit = 1; }

static void pidfile_path(char *out, size_t n, const char *name) {
    const char *rt = getenv("XDG_RUNTIME_DIR");
    snprintf(out, n, "%s/azoto-vscreen-%s.pid", rt ? rt : "/tmp", name);
}

static int stop(const char *name) {
    char path[512];
    pidfile_path(path, sizeof path, name);
    FILE *f = fopen(path, "r");
    if (!f) return 0; // nothing running
    int pid = 0;
    if (fscanf(f, "%d", &pid) != 1) pid = 0;
    fclose(f);
    char comm_path[64], comm[64] = "";
    snprintf(comm_path, sizeof comm_path, "/proc/%d/comm", pid);
    FILE *c = pid > 0 ? fopen(comm_path, "r") : NULL;
    if (c) { if (!fgets(comm, sizeof comm, c)) comm[0] = 0; fclose(c); }
    if (strncmp(comm, "azoto-vscreen", 13)) pid = 0; // stale pidfile: that pid is someone else now
    if (pid > 0 && kill(pid, SIGTERM) == 0) {
        for (int i = 0; i < 50 && kill(pid, 0) == 0; i++) usleep(100000);
        if (kill(pid, 0) == 0) kill(pid, SIGKILL); // KWin drops the screen with the connection
    }
    unlink(path);
    return 0;
}

// ready_fd: where to say "1" (screen up) or "0" (failed) when started in the background.
static int run(int width, int height, const char *name, int ready_fd) {
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "azoto-vscreen: no Wayland display (WAYLAND_DISPLAY=%s)\n", getenv("WAYLAND_DISPLAY"));
        return 1;
    }
    struct wl_registry *reg = wl_display_get_registry(display);
    wl_registry_add_listener(reg, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!screencast || screencast_version < 2) {
        fprintf(stderr, "azoto-vscreen: KWin doesn't offer virtual screens to this program "
                        "(needs ~/.local/share/applications/azoto-vscreen.desktop)\n");
        return 1;
    }
    struct zkde_screencast_stream_unstable_v1 *stream;
    if (screencast_version >= 4) {
        char desc[128];
        snprintf(desc, sizeof desc, "AzotoTV stream %dx%d", width, height);
        stream = zkde_screencast_unstable_v1_stream_virtual_output_with_description(
            screencast, name, desc, width, height, wl_fixed_from_int(1), ZKDE_SCREENCAST_UNSTABLE_V1_POINTER_HIDDEN);
    } else {
        stream = zkde_screencast_unstable_v1_stream_virtual_output(
            screencast, name, width, height, wl_fixed_from_int(1), ZKDE_SCREENCAST_UNSTABLE_V1_POINTER_HIDDEN);
    }
    zkde_screencast_stream_unstable_v1_add_listener(stream, &stream_listener, NULL);
    while (state == 0 && wl_display_dispatch(display) != -1) {}
    if (ready_fd >= 0) {
        if (write(ready_fd, state == 1 ? "1" : "0", 1) < 0) {}
        close(ready_fd);
    }
    if (state != 1) return 1;

    struct sigaction sa = {0};
    sa.sa_handler = on_signal;
    sigaction(SIGTERM, &sa, NULL);
    sigaction(SIGINT, &sa, NULL);
    sigaction(SIGHUP, &sa, NULL);
    // Own poll loop: wl_display_dispatch() retries on EINTR, so it would never see SIGTERM.
    while (!quit && state == 1) {
        while (wl_display_prepare_read(display) != 0) wl_display_dispatch_pending(display);
        wl_display_flush(display);
        struct pollfd p = { .fd = wl_display_get_fd(display), .events = POLLIN };
        int r = poll(&p, 1, -1);
        if (r > 0) {
            if (wl_display_read_events(display) == -1) break;
            wl_display_dispatch_pending(display);
        } else {
            wl_display_cancel_read(display);
            if (r < 0 && errno != EINTR) break;
        }
    }
    zkde_screencast_stream_unstable_v1_close(stream);
    wl_display_roundtrip(display);
    wl_display_disconnect(display);
    return 0;
}

static int parse_size(const char *s, int *w, int *h) {
    return sscanf(s, "%dx%d", w, h) == 2 && *w >= 320 && *h >= 200 && *w <= 7680 && *h <= 4320;
}

int main(int argc, char **argv) {
    const char *cmd = argc > 1 ? argv[1] : "";
    int w = 1920, h = 1080;
    const char *name = "AzotoTV";
    if (!strcmp(cmd, "stop")) return stop(argc > 2 ? argv[2] : name);
    if (strcmp(cmd, "start") && strcmp(cmd, "run")) {
        fprintf(stderr, "usage: azoto-vscreen start|run [WIDTHxHEIGHT] [NAME] | stop [NAME]\n");
        return 2;
    }
    if (argc > 2 && !parse_size(argv[2], &w, &h)) {
        fprintf(stderr, "azoto-vscreen: bad size %s\n", argv[2]);
        return 2;
    }
    if (argc > 3) name = argv[3];
    if (!strcmp(cmd, "run")) return run(w, h, name, -1);

    stop(name); // one screen per name
    int fds[2];
    if (pipe(fds)) return 1;
    pid_t pid = fork();
    if (pid < 0) return 1;
    if (pid == 0) {
        close(fds[0]);
        setsid();
        int null = open("/dev/null", O_RDWR);
        if (null >= 0) { dup2(null, 0); dup2(null, 1); }
        _exit(run(w, h, name, fds[1]));
    }
    close(fds[1]);
    char ok = '0';
    if (read(fds[0], &ok, 1) != 1) ok = '0';
    close(fds[0]);
    if (ok != '1') return 1;
    char path[512];
    pidfile_path(path, sizeof path, name);
    FILE *f = fopen(path, "w");
    if (f) { fprintf(f, "%d\n", pid); fclose(f); }
    return 0;
}
