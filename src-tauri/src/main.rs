// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK on the NVIDIA driver dies with "Error 71 (Protocol error)" when the
    // compositor offers explicit sync (KWin does). Must be set before GTK starts.
    #[cfg(target_os = "linux")]
    if std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
        // SAFETY: single-threaded at this point, nothing else reads the environment yet.
        unsafe { std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1") };
    }
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--system") {
        return azototv_lib::system_check();
    }
    if args.get(1).map(String::as_str) == Some("--arr-check") {
        return azototv_lib::arr_check(args.get(2).and_then(|n| n.parse().ok()).unwrap_or(3));
    }
    if args.get(1).map(String::as_str) == Some("--dl-check") {
        let media = args.get(2).map(String::as_str).unwrap_or("movie");
        return azototv_lib::dl_check(media, args.get(3).and_then(|n| n.parse().ok()).unwrap_or(0));
    }
    if args.get(1).map(String::as_str) == Some("--tv") {
        return azototv_lib::tv_cli(args.get(2).map(String::as_str).unwrap_or_default());
    }
    if args.get(1).map(String::as_str) == Some("--song-check") {
        let playlist = args.get(3).zip(args.get(4)).map(|(p, u)| (p.clone(), u.clone()));
        return azototv_lib::song_check(args.get(2).map(String::as_str).unwrap_or_default(), playlist);
    }
    if args.get(1).map(String::as_str) == Some("--3ds-check") {
        let a = |i: usize| args.get(i).cloned().unwrap_or_default();
        return azototv_lib::azahar_check(&a(2), &a(3), &a(4), a(5) == "split");
    }
    azototv_lib::run()
}
