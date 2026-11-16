//! `phone_rumble <secs>`: a phone-like ExternalPad that prints the rumble it
//! gets, inside a one-player xbox session (test: rumble the session's pad).
fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let _phone = azotopad::ExternalPad::new("Telefono di prova", |r| println!("PHONE rumble {r:?}")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let profile: azotopad::Profile = serde_json::from_str(r#"{"mode":"xbox"}"#).unwrap();
    let s = azotopad::Session::start(&profile, 1.0, 1, || {}).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(secs));
    s.stop();
}
