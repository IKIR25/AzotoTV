//! Interface sounds: a soft tick when the focus moves, a short rising blip on A
//! and a falling one on B. They're synthesized here (no sound files) and played
//! straight to the default output (ALSA -> PipeWire -> the TV's HDMI).
//!
//! An open output keeps the HDMI sink running, and the NVIDIA's HDMI audio
//! keeps the CPU package out of its deep idle state (~2 -> ~8.5 W), so it is
//! only open while someone is navigating: it opens on the first sound and
//! closes after `KEEP_OPEN` of silence; WirePlumber then suspends the sink 1 s
//! later (session/wireplumber-hdmi-suspend.conf, default 5 s). Closed, the
//! sound thread just sleeps on its channel.

use std::{
    f32::consts::TAU,
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

/// a pause in navigating longer than this closes the output (the next sound
/// opens it again; the TV may swallow the very start of that one)
const KEEP_OPEN: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sound {
    Move,
    Select,
    Back,
}

impl Sound {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "move" => Some(Self::Move),
            "select" => Some(Self::Select),
            "back" => Some(Self::Back),
            _ => None,
        }
    }

    /// (frequency Hz, start s, length s) per note, peak gain, decay time constant s
    fn recipe(self) -> (&'static [(f32, f32, f32)], f32, f32) {
        const C6: f32 = 1046.5;
        const G6: f32 = 1568.0;
        match self {
            Self::Move => (&[(1318.5, 0.0, 0.045)], 0.06, 0.010),
            Self::Select => (&[(C6, 0.0, 0.06), (G6, 0.045, 0.09)], 0.09, 0.028),
            Self::Back => (&[(G6, 0.0, 0.06), (C6, 0.045, 0.09)], 0.08, 0.028),
        }
    }

    /// Mono samples at `rate`: sine notes (plus a little octave for body),
    /// 2 ms attack, exponential decay, faded to 0 at the end so nothing clicks.
    fn render(self, rate: f32) -> Arc<[f32]> {
        let (notes, gain, tau) = self.recipe();
        let len = notes.iter().map(|&(_, s, l)| s + l).fold(0.0, f32::max);
        let mut out = vec![0.0; (len * rate) as usize + 1];
        for &(hz, start, dur) in notes {
            let (s0, n) = ((start * rate) as usize, (dur * rate) as usize);
            for i in 0..n {
                let t = i as f32 / rate;
                let env = (t / 0.002).min(1.0) * (-t / tau).exp() * ((n - i) as f32 / (0.005 * rate)).min(1.0);
                let w = (TAU * hz * t).sin() + 0.25 * (TAU * 2.0 * hz * t).sin();
                out[s0 + i] += gain * env * w / 1.25;
            }
        }
        out.into()
    }
}

static TX: OnceLock<Sender<Sound>> = OnceLock::new();

/// Play `sound` (never blocks: the sound thread does the work).
pub fn play(sound: Sound) {
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        if let Err(e) = std::thread::Builder::new().name("ui-sound".into()).spawn(move || run(rx)) {
            eprintln!("[sound] no thread: {e}");
        }
        tx
    });
    let _ = tx.send(sound);
}

fn run(rx: Receiver<Sound>) {
    let mut out: Option<Output> = None;
    let mut failed = false;
    loop {
        let sound = if out.is_some() {
            match rx.recv_timeout(KEEP_OPEN) {
                Ok(s) => s,
                Err(RecvTimeoutError::Timeout) => {
                    out = None;
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(s) => s,
                Err(_) => return,
            }
        };
        if out.is_none() {
            match Output::open() {
                Ok(o) => {
                    out = Some(o);
                    failed = false;
                }
                // logged once per failure streak (e.g. no sound card in a dev run)
                Err(e) if !failed => {
                    eprintln!("[sound] no output: {e}");
                    failed = true;
                }
                Err(_) => {}
            }
        }
        if let Some(o) = &out {
            o.play(sound);
        }
    }
}

struct Voice {
    samples: Arc<[f32]>,
    pos: usize,
}

struct Output {
    _stream: cpal::Stream,
    voices: Arc<Mutex<Vec<Voice>>>,
    bank: Vec<(Sound, Arc<[f32]>)>,
}

impl Output {
    fn open() -> Result<Self, String> {
        let device = cpal::default_host().default_output_device().ok_or("no output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        let rate = config.sample_rate.0 as f32;
        let voices = Arc::new(Mutex::new(Vec::<Voice>::new()));
        let v = voices.clone();
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, channels, v),
            SampleFormat::I16 => build::<i16>(&device, &config, channels, v),
            SampleFormat::I32 => build::<i32>(&device, &config, channels, v),
            SampleFormat::U16 => build::<u16>(&device, &config, channels, v),
            other => return Err(format!("sample format {other}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        let bank = [Sound::Move, Sound::Select, Sound::Back].map(|s| (s, s.render(rate))).to_vec();
        Ok(Self { _stream: stream, voices, bank })
    }

    fn play(&self, sound: Sound) {
        let Some((_, samples)) = self.bank.iter().find(|(s, _)| *s == sound) else { return };
        let mut v = self.voices.lock().unwrap();
        // a new tick replaces a tick still ringing (held D-pad: no pile-up)
        if sound == Sound::Move {
            v.retain(|x| !Arc::ptr_eq(&x.samples, samples));
        }
        v.push(Voice { samples: samples.clone(), pos: 0 });
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    voices: Arc<Mutex<Vec<Voice>>>,
) -> Result<cpal::Stream, String> {
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut v = voices.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    let mut s = 0.0;
                    for x in v.iter_mut() {
                        if let Some(&y) = x.samples.get(x.pos) {
                            s += y;
                            x.pos += 1;
                        }
                    }
                    frame.fill(T::from_sample(f32::clamp(s, -1.0, 1.0)));
                }
                v.retain(|x| x.pos < x.samples.len());
            },
            |e| eprintln!("[sound] stream: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}
