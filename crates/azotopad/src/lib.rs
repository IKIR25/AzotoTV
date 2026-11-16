//! Controller layer for games: while a game runs, the real pads are grabbed
//! (the game never sees them) and replaced by what the game's profile asks
//! for: a virtual Xbox 360 pad, or a keyboard and mouse driven by the pad
//! (like Steam Input). See [`profile`] for the profile format. Phones are
//! pads of their own ([`ExternalPad`]). Joy-Con / Pro Controller motion goes
//! to the Switch/Wii emulators over DSU ([`motion`]).

pub mod lights;
pub mod mapping;
pub mod motion;
pub mod profile;
pub mod session;

pub use profile::Profile;
pub use evdev::KeyCode;
pub use profile::key;
pub use mapping::{Btn, State, nintendo_letters};
pub use session::{EXTERNAL_PAD_ID, ExternalPad, PadId, Rumble, Session, Tapper, held_joycons, is_pad, set_stream_pads};
