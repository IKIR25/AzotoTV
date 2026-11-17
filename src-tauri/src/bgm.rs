//! Background music for the Mii app: the Mii makers' themes (Wii, 3DS, Wii U,
//! the user's own mp3s of the Switch 2 Mii Maker soundtrack, in
//! `<data>/mii/music/<track>.mp3`), looped forever at the exact sample, the
//! way the consoles play them. The files play their loop twice and fade out;
//! the loop points below were measured on them (the audio one loop later
//! matches to within mp3 noise), so the fade is never reached.
//!
//! The track is decoded once with ffmpeg (up to the loop's end) and played
//! with cpal to the default output, like the interface sounds (sound.rs). It
//! fades in, and fades out when stopped; then the output closes, so the HDMI
//! sink can sleep again.

use std::{
    path::PathBuf,
    process::Command,
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, SampleFormat, SizedSample,
};

/// The files' sample rate (loop points are in samples at this rate).
const FILE_RATE: u64 = 48_000;
/// (name, loop start, loop length), samples at `FILE_RATE`.
const TRACKS: [(&str, u64, u64); 3] = [
    ("wii", 240_000, 2_526_257),  // 5 s .. 57.630 s
    ("3ds", 240_000, 4_207_219),  // 5 s .. 92.650 s
    ("wiiu", 240_000, 3_474_259), // 5 s .. 77.380 s
];
/// The files are quiet already (peaks ~0.3, about -30 dB on average):
/// played as they are, soft background music.
const VOLUME: f32 = 1.0;
const FADE_IN: f32 = 1.2;
const FADE_OUT: f32 = 0.7;

enum Cmd {
    Play(String),
    Stop,
}

static TX: OnceLock<Sender<Cmd>> = OnceLock::new();

fn send(c: Cmd) {
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        if let Err(e) = std::thread::Builder::new().name("mii-music".into()).spawn(move || run(rx)) {
            eprintln!("[bgm] no thread: {e}");
        }
        tx
    });
    let _ = tx.send(c);
}

pub fn play(track: &str) -> Result<(), String> {
    if !TRACKS.iter().any(|t| t.0 == track) {
        return Err(format!("unknown track: {track}"));
    }
    send(Cmd::Play(track.to_string()));
    Ok(())
}

pub fn stop() {
    send(Cmd::Stop);
}

fn file(track: &str) -> PathBuf {
    crate::games::data_dir().join("mii/music").join(format!("{track}.mp3"))
}

/// What the stream plays: stereo samples up to the loop's end.
struct Voice {
    pcm: Vec<f32>,
    /// frame index (stereo pairs)
    pos: usize,
    start: usize,
    end: usize,
    gain: f32,
    target: f32,
    /// gain change per frame towards `target`
    step_in: f32,
    step_out: f32,
}

struct Playing {
    name: String,
    voice: Arc<Mutex<Voice>>,
    _stream: cpal::Stream,
}

fn run(rx: Receiver<Cmd>) {
    let mut now: Option<Playing> = None;
    loop {
        // while fading out, wake up to close the output once it's silent
        let cmd = match &now {
            Some(p) if p.voice.lock().unwrap().target == 0.0 => match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            _ => match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            },
        };
        match cmd {
            None => {
                if now.as_ref().is_some_and(|p| p.voice.lock().unwrap().gain <= 0.0) {
                    now = None;
                }
            }
            Some(Cmd::Stop) => {
                if let Some(p) = &now {
                    p.voice.lock().unwrap().target = 0.0;
                }
            }
            Some(Cmd::Play(name)) => {
                if let Some(p) = &now {
                    if p.name == name {
                        p.voice.lock().unwrap().target = VOLUME; // back in, where it was
                        continue;
                    }
                }
                now = None; // another track: this one stops (a short cut, not a fade)
                match start(&name) {
                    Ok(p) => now = Some(p),
                    Err(e) => eprintln!("[bgm] {name}: {e}"),
                }
            }
        }
    }
}

fn start(name: &str) -> Result<Playing, String> {
    let &(_, loop_start, loop_len) = TRACKS.iter().find(|t| t.0 == name).ok_or("no such track")?;
    let device = cpal::default_host().default_output_device().ok_or("no output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0 as u64;
    // the loop points at the output's rate (ffmpeg resamples if it isn't 48 kHz)
    let start = (loop_start * rate / FILE_RATE) as usize;
    let end = ((loop_start + loop_len) * rate / FILE_RATE) as usize;
    let pcm = decode(&file(name), rate, end)?;
    if pcm.len() / 2 < end {
        return Err(format!("the file is shorter than its loop ({} of {end} samples)", pcm.len() / 2));
    }
    let r = rate as f32;
    let voice = Arc::new(Mutex::new(Voice {
        pcm,
        pos: 0,
        start,
        end,
        gain: 0.0,
        target: VOLUME,
        step_in: VOLUME / (FADE_IN * r),
        step_out: VOLUME / (FADE_OUT * r),
    }));
    let channels = config.channels as usize;
    let v = voice.clone();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, v),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, v),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, v),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, v),
        other => return Err(format!("sample format {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Playing { name: name.to_string(), voice, _stream: stream })
}

/// Stereo f32 samples at `rate`, the first `frames` of them.
fn decode(path: &std::path::Path, rate: u64, frames: usize) -> Result<Vec<f32>, String> {
    let secs = frames as f64 / rate as f64 + 0.05;
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-t", &format!("{secs:.3}"), "-ac", "2", "-ar", &rate.to_string(), "-f", "f32le", "-"])
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    voice: Arc<Mutex<Voice>>,
) -> Result<cpal::Stream, String> {
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut v = voice.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    if v.gain < v.target {
                        v.gain = (v.gain + v.step_in).min(v.target);
                    } else if v.gain > v.target {
                        v.gain = (v.gain - v.step_out).max(v.target);
                    }
                    let (l, r) = (v.pcm[2 * v.pos] * v.gain, v.pcm[2 * v.pos + 1] * v.gain);
                    v.pos += 1;
                    if v.pos >= v.end {
                        v.pos = v.start; // the loop: same audio, one loop earlier
                    }
                    match frame {
                        [m] => *m = T::from_sample((l + r) / 2.0),
                        [a, b, rest @ ..] => {
                            *a = T::from_sample(l);
                            *b = T::from_sample(r);
                            rest.fill(T::from_sample(0.0));
                        }
                        [] => {}
                    }
                }
            },
            |e| eprintln!("[bgm] stream: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loop seam with the real files (AZOTOTV_MII_MUSIC = their folder):
    /// jumping back sounds like going on, within mp3 noise.
    #[test]
    fn loops_are_seamless() {
        let Some(dir) = std::env::var_os("AZOTOTV_MII_MUSIC").map(PathBuf::from) else { return };
        for (name, start, len) in TRACKS {
            let pcm = decode(&dir.join(format!("{name}.mp3")), FILE_RATE, (start + len) as usize + 48_000).unwrap();
            let (s, e) = (2 * start as usize, 2 * (start + len) as usize);
            let n = 2 * 4800; // 100 ms
            // what plays after the seam (the loop's start) vs what the file has there
            let diff: f32 = (0..n).map(|i| (pcm[s + i] - pcm[e + i]).powi(2)).sum::<f32>();
            let level: f32 = (0..n).map(|i| pcm[e + i].powi(2)).sum::<f32>();
            let err = (diff / level).sqrt();
            assert!(err < 0.1, "{name}: {err}");
        }
    }
}
