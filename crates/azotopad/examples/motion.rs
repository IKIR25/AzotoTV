//! Serves the motion of one or two IMU devices as player 1 for a while:
//! `motion <main imu> [left imu] [seconds]` (a DSU client can then read it).
use std::{path::PathBuf, time::Duration};

use azotopad::motion::{Imu, Motion, Sensors, SharedSensors};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let imu = |p: &String| Imu { path: PathBuf::from(p), flip: false };
    let secs = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let sensors = SharedSensors::default();
    *sensors.lock().unwrap() = vec![Sensors { main: a.first().map(imu), left: a.get(1).map(imu) }];
    let _m = Motion::start(sensors).expect("motion");
    std::thread::sleep(Duration::from_secs(secs));
}
