//! Motion (gyroscope + accelerometer) of each player's controller, for the
//! Switch and Wii emulators: the game only sees azotopad's virtual Xbox pads,
//! which have no motion, so the real controllers' motion goes to the
//! emulator over DSU (the "cemuhook" UDP protocol Eden and Dolphin both read).
//!
//! Two servers on localhost, 4 slots each, slot = player:
//! - [`PORT`]: the player's controller (the right Joy-Con of a pair, a single
//!   Joy-Con, a Pro Controller) = Switch motion, Wii Remote.
//! - [`LEFT_PORT`]: the left Joy-Con of a pair = the Nunchuk.
//!
//! The motion comes from the controller's own IMU device (hid-nintendo makes
//! one next to each Joy-Con / Pro Controller, with the same MAC as `uniq`);
//! the session says which IMU belongs to which player ([`Sensors`]).

use std::{
    collections::HashMap,
    io,
    net::{SocketAddr, UdpSocket},
    os::fd::AsRawFd,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant, UNIX_EPOCH},
};

use evdev::{AbsoluteAxisCode as Abs, EventType, raw_stream::RawDevice};

pub const PORT: u16 = 26760;
pub const LEFT_PORT: u16 = 26761;
const SLOTS: usize = 4;
/// a client that hasn't asked for data again in this long is gone (the protocol's 5 s)
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
/// A Joy-Con sends ~66 samples/s; a weak Bluetooth link stops it for longer
/// now and then. Dolphin keeps applying the last rotation until the next
/// sample (the pointer slides on its own), so after this long without one
/// it is told the controller stopped turning.
const STALE: Duration = Duration::from_millis(40);

const VERSION: u16 = 1001;
const MSG_VERSION: u32 = 0x100000;
const MSG_PORTS: u32 = 0x100001;
const MSG_DATA: u32 = 0x100002;

/// One IMU device.
#[derive(Clone, Debug, PartialEq)]
pub struct Imu {
    pub path: PathBuf,
    /// a single right Joy-Con held sideways: turned half around, so both
    /// sideways Joy-Cons move like a Wii Remote held sideways (whose top is on
    /// the left, like the left Joy-Con's)
    pub flip: bool,
}

/// Where a player's motion comes from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sensors {
    pub main: Option<Imu>,
    /// the left Joy-Con of a pair
    pub left: Option<Imu>,
}

/// Every player's [`Sensors`], kept up to date by the session.
pub type SharedSensors = Arc<Mutex<Vec<Sensors>>>;

pub struct Motion {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Motion {
    pub fn start(sensors: SharedSensors) -> io::Result<Motion> {
        let socks = [UdpSocket::bind(("127.0.0.1", PORT))?, UdpSocket::bind(("127.0.0.1", LEFT_PORT))?];
        for s in &socks {
            s.set_nonblocking(true)?;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new().name("azotopad-motion".into()).spawn(move || {
            let mut srv = Server {
                socks,
                clients: Default::default(),
                imus: HashMap::new(),
                want: Vec::new(),
                counter: [[0; SLOTS]; 2],
            };
            while !stop2.load(Ordering::Relaxed) {
                let now = sensors.lock().unwrap().clone();
                if now != srv.want {
                    srv.set(now);
                }
                srv.poll(STALE.as_millis() as i32 / 2);
            }
        })?;
        Ok(Motion { stop, thread: Some(thread) })
    }
}

impl Drop for Motion {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Device {
    dev: RawDevice,
    mac: [u8; 6],
    /// raw units per g and per °/s
    accel_res: f32,
    gyro_res: f32,
    /// raw ABS_X/Y/Z (accel) and ABS_RX/RY/RZ (gyro)
    raw: [i32; 6],
    /// µs, of the last sample
    time: u64,
    /// when the last sample came, and whether "not turning" was sent since
    last: Instant,
    stilled: bool,
}

impl Device {
    fn open(path: &PathBuf) -> io::Result<Device> {
        let dev = RawDevice::open(path)?;
        unsafe {
            let fd = dev.as_raw_fd();
            libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
        }
        let (mut accel_res, mut gyro_res) = (4096.0, 14247.0); // hid-nintendo's
        for (a, info) in dev.get_absinfo()? {
            let r = info.resolution() as f32;
            if r > 0.0 {
                match a {
                    Abs::ABS_X => accel_res = r,
                    Abs::ABS_RX => gyro_res = r,
                    _ => {}
                }
            }
        }
        let mut mac = [0u8; 6];
        if let Some(u) = dev.unique_name() {
            for (i, b) in u.split(':').take(6).enumerate() {
                mac[i] = u8::from_str_radix(b, 16).unwrap_or(0);
            }
        }
        Ok(Device { dev, mac, accel_res, gyro_res, raw: [0; 6], time: 0, last: Instant::now(), stilled: true })
    }

    fn sample(&self, flip: bool) -> [f32; 6] {
        let mut s = to_dsu(self.raw, self.accel_res, self.gyro_res, flip);
        if self.stilled {
            s[3..].fill(0.0);
        }
        s
    }
}

/// In DSU's axes (the DualShock 4's: accel x, y, z in g, gyro pitch, yaw,
/// roll in °/s) from hid-nintendo's (the same for both Joy-Cons and the Pro
/// Controller), as joycond-cemuhook does.
fn to_dsu(raw: [i32; 6], accel_res: f32, gyro_res: f32, flip: bool) -> [f32; 6] {
    let a = |i: usize| raw[i] as f32 / accel_res;
    let g = |i: usize| raw[i + 3] as f32 / gyro_res;
    let s = [a(1), -a(2), a(0), -g(1), -g(2), g(0)];
    // half a turn around the vertical axis (y, yaw)
    if flip { [-s[0], s[1], -s[2], -s[3], s[4], -s[5]] } else { s }
}

#[derive(Default)]
struct Client {
    last: Option<Instant>,
    /// slots it asked for (all of them, or some)
    slots: [bool; SLOTS],
}

struct Server {
    /// [main, left]
    socks: [UdpSocket; 2],
    clients: [HashMap<SocketAddr, Client>; 2],
    imus: HashMap<PathBuf, Device>,
    want: Vec<Sensors>,
    counter: [[u32; SLOTS]; 2],
}

impl Server {
    /// The IMU of server `s`, slot `slot`.
    fn imu(&self, s: usize, slot: usize) -> Option<&Imu> {
        let p = self.want.get(slot)?;
        if s == 0 { p.main.as_ref() } else { p.left.as_ref() }
    }

    fn set(&mut self, want: Vec<Sensors>) {
        let used: Vec<PathBuf> =
            want.iter().flat_map(|s| [&s.main, &s.left]).flatten().map(|i| i.path.clone()).collect();
        self.imus.retain(|p, _| used.contains(p));
        for p in used {
            if !self.imus.contains_key(&p) {
                match Device::open(&p) {
                    Ok(d) => {
                        eprintln!("[azotopad] motion from {}", p.display());
                        self.imus.insert(p, d);
                    }
                    Err(e) => eprintln!("[azotopad] motion: cannot open {}: {e}", p.display()),
                }
            }
        }
        self.want = want;
    }

    fn poll(&mut self, timeout_ms: i32) {
        let paths: Vec<PathBuf> = self.imus.keys().cloned().collect();
        let mut fds: Vec<libc::pollfd> = self
            .socks
            .iter()
            .map(|s| s.as_raw_fd())
            .chain(paths.iter().map(|p| self.imus[p].dev.as_raw_fd()))
            .map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 })
            .collect();
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        for s in 0..2 {
            if fds[s].revents & libc::POLLIN != 0 {
                self.requests(s);
            }
        }
        for (i, path) in paths.iter().enumerate() {
            let rev = fds[2 + i].revents;
            if rev & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                // gone (switched off); the session notices and moves on
                self.imus.remove(path);
                continue;
            }
            if rev & libc::POLLIN != 0 && self.read(path) {
                self.send(path);
            }
        }
        let stale: Vec<PathBuf> =
            self.imus.iter().filter(|(_, d)| !d.stilled && d.last.elapsed() >= STALE).map(|(p, _)| p.clone()).collect();
        for p in stale {
            if let Some(d) = self.imus.get_mut(&p) {
                d.stilled = true;
            }
            self.send(&p);
        }
    }

    /// Applies the IMU's pending events; true when a new sample is complete.
    fn read(&mut self, path: &PathBuf) -> bool {
        let Some(d) = self.imus.get_mut(path) else { return false };
        let mut done = false;
        let Ok(evs) = d.dev.fetch_events() else { return false };
        for ev in evs {
            if ev.event_type() == EventType::ABSOLUTE {
                let i = match Abs(ev.code()) {
                    Abs::ABS_X => 0,
                    Abs::ABS_Y => 1,
                    Abs::ABS_Z => 2,
                    Abs::ABS_RX => 3,
                    Abs::ABS_RY => 4,
                    Abs::ABS_RZ => 5,
                    _ => continue,
                };
                d.raw[i] = ev.value();
            } else if ev.event_type() == EventType::SYNCHRONIZATION {
                d.time = ev.timestamp().duration_since(UNIX_EPOCH).map(|t| t.as_micros() as u64).unwrap_or(0);
                d.last = Instant::now();
                d.stilled = false;
                done = true;
            }
        }
        done
    }

    /// The new sample of `path` to every client of the slots it feeds.
    fn send(&mut self, path: &PathBuf) {
        let now = Instant::now();
        for s in 0..2 {
            for slot in 0..SLOTS {
                let Some(flip) = self.imu(s, slot).filter(|i| &i.path == path).map(|i| i.flip) else { continue };
                let d = &self.imus[path];
                let targets: Vec<SocketAddr> = self.clients[s]
                    .iter()
                    .filter(|(_, c)| c.slots[slot] && c.last.is_some_and(|t| now - t < CLIENT_TIMEOUT))
                    .map(|(a, _)| *a)
                    .collect();
                if targets.is_empty() {
                    continue;
                }
                self.counter[s][slot] = self.counter[s][slot].wrapping_add(1);
                let msg = pad_data(slot as u8, d.mac, self.counter[s][slot], d.time, d.sample(flip));
                for a in targets {
                    let _ = self.socks[s].send_to(&msg, a);
                }
            }
        }
        for c in &mut self.clients {
            c.retain(|_, c| c.last.is_some_and(|t| now - t < CLIENT_TIMEOUT));
        }
    }

    fn requests(&mut self, s: usize) {
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = self.socks[s].recv_from(&mut buf) {
            let msg = &buf[..n];
            if n < 20 || &msg[..4] != b"DSUC" {
                continue;
            }
            match u32::from_le_bytes(msg[16..20].try_into().unwrap()) {
                MSG_VERSION => {
                    let _ = self.socks[s].send_to(&packet(MSG_VERSION, &[&VERSION.to_le_bytes()[..], &[0, 0]].concat()), from);
                }
                MSG_PORTS if n >= 24 => {
                    let count = (i32::from_le_bytes(msg[20..24].try_into().unwrap()).max(0) as usize).min(n - 24);
                    for &slot in &msg[24..24 + count] {
                        let slot = slot as usize;
                        if slot >= SLOTS {
                            continue;
                        }
                        let dev = self.imu(s, slot).and_then(|i| self.imus.get(&i.path));
                        let mut body = shared(slot as u8, dev.map(|d| d.mac));
                        body.push(0);
                        let _ = self.socks[s].send_to(&packet(MSG_PORTS, &body), from);
                    }
                }
                MSG_DATA if n >= 22 => {
                    let c = self.clients[s].entry(from).or_default();
                    c.last = Some(Instant::now());
                    match msg[20] {
                        1 => {
                            if let Some(x) = c.slots.get_mut(msg[21] as usize) {
                                *x = true;
                            }
                        }
                        // all pads, or by MAC (each slot is one controller anyway)
                        _ => c.slots = [true; SLOTS],
                    }
                }
                _ => {}
            }
        }
    }
}

/// The start every port-info and pad-data reply shares: slot, state, model,
/// connection, MAC, battery. `None` = nothing in that slot.
fn shared(slot: u8, mac: Option<[u8; 6]>) -> Vec<u8> {
    let mut v = vec![slot];
    match mac {
        // connected, full gyro, Bluetooth
        Some(m) => {
            v.extend([2, 2, 2]);
            v.extend(m);
        }
        None => v.extend([0u8; 9]),
    }
    v.push(0); // battery: not reported
    v
}

fn pad_data(slot: u8, mac: [u8; 6], counter: u32, time_us: u64, m: [f32; 6]) -> Vec<u8> {
    let mut v = shared(slot, Some(mac));
    v.push(1); // active
    v.extend(counter.to_le_bytes());
    // buttons, home, touch: none (the game gets them from the virtual pad)
    v.extend([0u8; 4]);
    // sticks centred
    v.extend([128u8; 4]);
    // analog buttons and triggers
    v.extend([0u8; 12]);
    // two touches
    v.extend([0u8; 12]);
    v.extend(time_us.to_le_bytes());
    for f in m {
        v.extend(f.to_le_bytes());
    }
    packet(MSG_DATA, &v)
}

/// Header + message type + body, with its CRC.
fn packet(kind: u32, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(20 + body.len());
    v.extend(b"DSUS");
    v.extend(VERSION.to_le_bytes());
    v.extend(((4 + body.len()) as u16).to_le_bytes());
    v.extend([0u8; 4]); // CRC, below
    v.extend(0x4e54_0001u32.to_le_bytes()); // server id
    v.extend(kind.to_le_bytes());
    v.extend(body);
    let crc = crc32(&v);
    v[8..12].copy_from_slice(&crc.to_le_bytes());
    v
}

/// CRC-32 (IEEE, as zlib).
fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_is_zlibs() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn pad_data_is_100_bytes() {
        let p = pad_data(1, [1, 2, 3, 4, 5, 6], 7, 8, [0.0; 6]);
        assert_eq!(p.len(), 100);
        assert_eq!(&p[..4], b"DSUS");
        assert_eq!(u16::from_le_bytes([p[6], p[7]]) as usize, p.len() - 16);
        let mut z = p.clone();
        z[8..12].fill(0);
        assert_eq!(crc32(&z).to_le_bytes(), p[8..12]);
        assert_eq!(p[20], 1); // slot
        assert_eq!(p[21], 2); // connected
    }

    #[test]
    fn port_info_is_32_bytes() {
        let mut b = shared(3, None);
        b.push(0);
        assert_eq!(packet(MSG_PORTS, &b).len(), 32);
    }

    #[test]
    fn flip_turns_half_around() {
        let raw = [1, 2, 3, 4, 5, 6];
        assert_eq!(to_dsu(raw, 1.0, 1.0, false), [2.0, -3.0, 1.0, -5.0, -6.0, 4.0]);
        assert_eq!(to_dsu(raw, 1.0, 1.0, true), [-2.0, -3.0, -1.0, 5.0, -6.0, -4.0]);
    }
}
