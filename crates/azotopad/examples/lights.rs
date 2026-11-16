//! `lights <event node> <player|0>`: shows a player number on a controller.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let player: usize = args[1].parse().unwrap();
    azotopad::lights::show(std::path::Path::new(&args[0]), (player > 0).then_some(player));
}
