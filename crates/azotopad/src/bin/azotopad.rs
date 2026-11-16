//! Test wrapper: `azotopad <profile> -- <game command...>`
//! <profile> is `xbox`, a JSON file, or inline JSON. The pads are mapped while
//! the game runs; holding the guide button (or back+start) for 1 s closes it.
//! AZOTOPAD_SENS (default 1.0) scales the cursor speed, AZOTOPAD_PLAYERS (default 4)
//! caps the players (1 = every pad is player 1).

use std::{
    os::unix::process::CommandExt,
    process::{Command, exit},
    sync::{Arc, atomic::{AtomicI32, Ordering}},
    time::Duration,
};

use azotopad::{Profile, Session};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(sep) = args.iter().position(|a| a == "--") else {
        eprintln!("usage: azotopad <xbox|profile.json|'{{json}}'> -- <command...>");
        exit(2);
    };
    if sep != 1 || sep + 1 >= args.len() {
        eprintln!("usage: azotopad <xbox|profile.json|'{{json}}'> -- <command...>");
        exit(2);
    }
    let spec = &args[0];
    let json = if spec == "xbox" {
        r#"{"mode":"xbox"}"#.to_string()
    } else if spec.trim_start().starts_with('{') {
        spec.clone()
    } else {
        std::fs::read_to_string(spec).unwrap_or_else(|e| { eprintln!("{spec}: {e}"); exit(2) })
    };
    let profile: Profile = serde_json::from_str(&json).unwrap_or_else(|e| { eprintln!("profile: {e}"); exit(2) });

    let pgid = Arc::new(AtomicI32::new(0));
    let pg = pgid.clone();
    let sens = std::env::var("AZOTOPAD_SENS").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let players = std::env::var("AZOTOPAD_PLAYERS").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    let session = Session::start(&profile, sens, players, move || {
        let g = pg.load(Ordering::Relaxed);
        if g > 0 {
            eprintln!("[azotopad] quit requested, closing the game");
            unsafe { libc::kill(-g, libc::SIGTERM) };
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                unsafe { libc::kill(-g, libc::SIGKILL) };
            });
        }
    })
    .unwrap_or_else(|e| { eprintln!("azotopad: {e}"); exit(1) });
    // let udev publish the virtual devices before the game looks for pads
    std::thread::sleep(Duration::from_millis(300));

    let ignore: Vec<String> = session.pads().iter().map(|(v, p)| format!("0x{v:04x}/0x{p:04x}")).collect();
    let mut cmd = Command::new(&args[2]);
    cmd.args(&args[3..]);
    if !ignore.is_empty() {
        cmd.env("SDL_GAMECONTROLLER_IGNORE_DEVICES", ignore.join(","));
    }
    unsafe { cmd.pre_exec(|| { libc::setsid(); Ok(()) }) };
    let mut child = cmd.spawn().unwrap_or_else(|e| { eprintln!("{}: {e}", args[2]); exit(1) });
    pgid.store(child.id() as i32, Ordering::Relaxed);
    let status = child.wait();
    session.stop();
    exit(status.ok().and_then(|s| s.code()).unwrap_or(1));
}
