//! The Mii app's storage: each profile's own Mii library, and the consoles'
//! Mii databases it is synced with.
//!
//! - Library: `players/<id>/miis.json`, `{"version": 1, "miis": [...]}`. The
//!   webview owns what a Mii looks like (miijs does the format work there); it
//!   only has to carry an `id`, and the per-console encodings the sync writes:
//!   `nx` (Switch StoreData, 0x44 bytes) and `rvl` (Wii RFLCharData, 74 bytes),
//!   base64.
//! - Switch: Eden's `nand/system/save/8000000000000030/MiiDatabase.dat`
//!   ("NFDB": 100 StoreData slots, version, count, CRC16).
//! - Wii: the player's Dolphin NAND `shared2/menu/FaceLib/RFL_DB.dat` ("RNOD":
//!   100 RFLCharData slots, then the "RNHD" hidden Miis, CRC16 at 0x1F1DE).
//!
//! Both CRCs are CRC-16/XMODEM (poly 0x1021, init 0, big-endian), checked
//! against the files Eden and Dolphin wrote.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};

use crate::games::player_dir;

// ---- CRC ---------------------------------------------------------------------

pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

// ---- Switch: NFDB ------------------------------------------------------------

pub const NX_SLOTS: usize = 100;
pub const NX_ENTRY: usize = 0x44;
const NX_LEN: usize = 4 + NX_SLOTS * NX_ENTRY + 4;
const NX_VERSION: usize = 4 + NX_SLOTS * NX_ENTRY;
const NX_COUNT: usize = NX_VERSION + 1;
/// The StoreData device CRC Eden writes: it only depends on the console's
/// device id (the data is never mixed in), so it's the same for every Mii.
const EDEN_DEVICE_CRC: [u8; 2] = [0xc8, 0xd6];

pub fn eden_db() -> PathBuf {
    crate::eden::eden_data().join("nand/system/save/8000000000000030/MiiDatabase.dat")
}

/// The Miis in an NFDB, in order.
pub fn nx_entries(db: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if db.len() != NX_LEN || &db[..4] != b"NFDB" {
        return Err("not a Switch Mii database".into());
    }
    let n = (db[NX_COUNT] as usize).min(NX_SLOTS);
    Ok((0..n).map(|i| db[4 + i * NX_ENTRY..4 + (i + 1) * NX_ENTRY].to_vec()).collect())
}

/// An NFDB holding `entries` (StoreData, 0x44 bytes each). Each entry gets its
/// data CRC recomputed and the device CRC of `device_crc_from` (the database
/// it replaces) when that has any entry, Eden's otherwise.
pub fn nx_build(entries: &[Vec<u8>], device_crc_from: Option<&[u8]>) -> Result<Vec<u8>, String> {
    if entries.len() > NX_SLOTS {
        return Err(format!("a Switch holds {NX_SLOTS} Miis at most"));
    }
    let device = device_crc_from
        .and_then(|db| nx_entries(db).ok())
        .and_then(|e| e.first().map(|e| [e[0x42], e[0x43]]))
        .unwrap_or(EDEN_DEVICE_CRC);
    let mut db = vec![0u8; NX_LEN];
    db[..4].copy_from_slice(b"NFDB");
    for (i, e) in entries.iter().enumerate() {
        if e.len() != NX_ENTRY {
            return Err(format!("Switch Mii {i}: {} bytes, expected {NX_ENTRY}", e.len()));
        }
        let slot = &mut db[4 + i * NX_ENTRY..4 + (i + 1) * NX_ENTRY];
        slot[..0x40].copy_from_slice(&e[..0x40]);
        slot[0x40..0x42].copy_from_slice(&crc16(&e[..0x40]).to_be_bytes());
        slot[0x42..].copy_from_slice(&device);
    }
    db[NX_VERSION] = 1;
    db[NX_COUNT] = entries.len() as u8;
    let crc = crc16(&db[..NX_LEN - 2]);
    db[NX_LEN - 2..].copy_from_slice(&crc.to_be_bytes());
    Ok(db)
}

// ---- Wii: RFL_DB -------------------------------------------------------------

pub const RVL_SLOTS: usize = 100;
pub const RVL_ENTRY: usize = 74;
const RVL_LEN: usize = 0xBE6C0;
const RVL_CRC: usize = 0x1F1DE;

pub fn rvl_db(player: &Path) -> PathBuf {
    player.join("wii/shared2/menu/FaceLib/RFL_DB.dat")
}

/// A Wii Mii database with no Miis, byte for byte what the Wii Menu (Dolphin)
/// creates: the "RNHD" hidden list has 10000 empty records.
pub fn rvl_blank() -> Vec<u8> {
    let mut db = vec![0u8; RVL_LEN];
    db[..4].copy_from_slice(b"RNOD");
    db[0x1CEC] = 0x80;
    db[0x1D00..0x1D08].copy_from_slice(b"RNHD\xff\xff\xff\xff");
    for i in 0..10000 {
        let at = 0x1D10 + i * 12;
        db[at..at + 4].copy_from_slice(&[0x7f, 0xff, 0x7f, 0xff]);
    }
    let crc = crc16(&db[..RVL_CRC]);
    db[RVL_CRC..RVL_CRC + 2].copy_from_slice(&crc.to_be_bytes());
    db
}

/// The Miis in an RFL_DB (empty slots skipped).
pub fn rvl_entries(db: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if db.len() != RVL_LEN || &db[..4] != b"RNOD" {
        return Err("not a Wii Mii database".into());
    }
    Ok((0..RVL_SLOTS)
        .map(|i| &db[4 + i * RVL_ENTRY..4 + (i + 1) * RVL_ENTRY])
        .filter(|e| e.iter().any(|&b| b != 0))
        .map(<[u8]>::to_vec)
        .collect())
}

/// `base` (the player's database, or a blank one) with its Miis replaced by
/// `entries`; everything else (the hidden Miis games keep) stays.
pub fn rvl_build(base: Option<&[u8]>, entries: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if entries.len() > RVL_SLOTS {
        return Err(format!("a Wii holds {RVL_SLOTS} Miis at most"));
    }
    let mut db = match base {
        Some(b) if b.len() == RVL_LEN && &b[..4] == b"RNOD" => b.to_vec(),
        _ => rvl_blank(),
    };
    db[4..4 + RVL_SLOTS * RVL_ENTRY].fill(0);
    for (i, e) in entries.iter().enumerate() {
        if e.len() != RVL_ENTRY {
            return Err(format!("Wii Mii {i}: {} bytes, expected {RVL_ENTRY}", e.len()));
        }
        db[4 + i * RVL_ENTRY..4 + (i + 1) * RVL_ENTRY].copy_from_slice(e);
    }
    let crc = crc16(&db[..RVL_CRC]);
    db[RVL_CRC..RVL_CRC + 2].copy_from_slice(&crc.to_be_bytes());
    Ok(db)
}

/// Nintendo's Mii models and textures for drawing (the Wii U's FFLResHigh.dat,
/// system title 0005001B10056000): `$AZOTOTV_FFL_RES`, else `mii/FFLResHigh.dat`
/// in AzotoTV's data folder. Not shipped with AzotoTV.
pub fn ffl_resource() -> Result<Vec<u8>, String> {
    let path = std::env::var_os("AZOTOTV_FFL_RES")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::games::data_dir().join("mii/FFLResHigh.dat"));
    fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

type Entries = fn(&[u8]) -> Result<Vec<Vec<u8>>, String>;

// ---- the library -------------------------------------------------------------

/// One writer at a time (TV and phone can both save).
static LOCK: Mutex<()> = Mutex::new(());

fn library_path(user: &str) -> PathBuf {
    player_dir(user).join("miis.json")
}

fn load(user: &str) -> Value {
    fs::read_to_string(library_path(user))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v["miis"].is_array())
        .map(|mut v| {
            if !v["people"].is_array() {
                v["people"] = json!([]);
            }
            v
        })
        .unwrap_or_else(|| json!({ "version": 2, "people": [], "miis": [] }))
}

fn store(user: &str, lib: &Value) -> Result<(), String> {
    let path = library_path(user);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.new");
    fs::write(&tmp, serde_json::to_string_pretty(lib).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

fn check_user(user: &str) -> Result<(), String> {
    if user.is_empty() || !user.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("no profile".into());
    }
    Ok(())
}

pub fn library(user: &str) -> Result<Value, String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    Ok(load(user))
}

/// Adds the people (names, birthday, family; their Miis point at them), or
/// replaces those with the same `id`.
pub fn put_people(user: &str, people: Vec<Value>) -> Result<(), String> {
    check_user(user)?;
    if people.iter().any(|p| p["id"].as_str().is_none_or(str::is_empty)) {
        return Err("a person needs an id".into());
    }
    let _g = LOCK.lock().unwrap();
    let mut lib = load(user);
    upsert(lib["people"].as_array_mut().unwrap(), people);
    lib["version"] = json!(2);
    store(user, &lib)
}

/// A person and all their Miis.
pub fn remove_person(user: &str, id: &str) -> Result<(), String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    let mut lib = load(user);
    lib["people"].as_array_mut().unwrap().retain(|p| p["id"] != id);
    lib["miis"].as_array_mut().unwrap().retain(|m| m["person"] != id);
    store(user, &lib)
}

fn upsert(list: &mut Vec<Value>, items: Vec<Value>) {
    for m in items {
        match list.iter_mut().find(|o| o["id"] == m["id"]) {
            Some(o) => *o = m,
            None => list.push(m),
        }
    }
}

/// Adds the Miis, or replaces those with the same `id`.
pub fn put(user: &str, miis: Vec<Value>) -> Result<(), String> {
    check_user(user)?;
    for m in &miis {
        if m["id"].as_str().is_none_or(str::is_empty) {
            return Err("a Mii needs an id".into());
        }
        for (key, len) in [("nx", NX_ENTRY), ("rvl", RVL_ENTRY)] {
            if let Some(s) = m[key].as_str() {
                let n = B64.decode(s).map_err(|e| format!("{key}: {e}"))?.len();
                if n != len {
                    return Err(format!("{key}: {n} bytes, expected {len}"));
                }
            }
        }
    }
    let _g = LOCK.lock().unwrap();
    let mut lib = load(user);
    upsert(lib["miis"].as_array_mut().unwrap(), miis);
    store(user, &lib)
}

pub fn remove(user: &str, id: &str) -> Result<(), String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    let mut lib = load(user);
    lib["miis"].as_array_mut().unwrap().retain(|m| m["id"] != id);
    store(user, &lib)
}

/// The order of the profile's Miis (ids); unknown ids are ignored, missing
/// ones keep their place after the listed ones.
pub fn reorder(user: &str, ids: &[String]) -> Result<(), String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    let mut lib = load(user);
    let list = lib["miis"].as_array_mut().unwrap();
    let pos = |m: &Value| ids.iter().position(|i| m["id"] == **i).unwrap_or(usize::MAX);
    list.sort_by_key(pos);
    store(user, &lib)
}

/// What the consoles hold, for importing: the Switch Miis every profile
/// shared before AzotoTV kept them per profile (Eden's, if it never synced)
/// and this player's Wii Miis, raw, base64.
pub fn sources(user: &str) -> Result<Value, String> {
    check_user(user)?;
    let read = |p: PathBuf, f: Entries| -> Vec<String> {
        fs::read(p)
            .ok()
            .and_then(|d| f(&d).ok())
            .unwrap_or_default()
            .iter()
            .map(|e| B64.encode(e))
            .collect()
    };
    let saves = crate::eden::save_dir(&player_dir(user));
    let game = |title: &str, file: &str| saves.as_ref().and_then(|d| fs::read(d.join(title).join(file)).ok());
    Ok(json!({
        "switch": read(if eden_shared().exists() { eden_shared() } else { eden_db() }, nx_entries),
        "wii": read(rvl_db(&player_dir(user)), rvl_entries),
        "miitopia": game(MIITOPIA, "main.sav").map(|s| miitopia_miis(&s).iter().map(|e| B64.encode(e)).collect::<Vec<_>>()).unwrap_or_default(),
        "tomodachi": game(TOMODACHI, "Mii.sav").map(|s| tomodachi_miis(&s)).unwrap_or_default(),
    }))
}

// ---- Miis in game saves (read only) ----------------------------------------------

const MIITOPIA: &str = "01003DA010E8A000";
const TOMODACHI: &str = "010051F0207B2000";

/// Miitopia's cast (`main.sav`): a Switch CharInfo (0x58 bytes) every 0x800
/// from 0x2fd8. A Mii playing several roles is there several times, each copy
/// with a new create ID: once here, the first copy (the one with the ID the
/// Mii had on the Switch).
pub fn miitopia_miis(save: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![];
    for k in 0..100 {
        let at = 0x2fd8 + k * 0x800;
        let Some(ci) = save.get(at..at + 0x58) else { break };
        let named = ci[0x10..0x12] != [0, 0];
        if ci[..16].iter().all(|&b| b == 0) || !named || ci[0x28] > 1 || out.iter().any(|o| o[0x10..] == ci[0x10..]) {
            continue;
        }
        out.push(ci.to_vec());
    }
    out
}

/// Tomodachi Life: Living the Dream's `Mii.sav`: a table of (key hash, value
/// or offset) pairs up to the data, hash 0 starting a run of one type; the
/// keys are the ones the ltd-save-editor project worked out.
mod ltd {
    pub const CHAR_INFO_EX: u32 = 0x881ca27a; // BinaryArray of 0x98-byte CharInfoEx
    pub const NAME: u32 = 0x2499bfda; // WString32Array (64 bytes each)
    pub const HOW_TO_CALL: u32 = 0x3a5eda05; // WString64Array (128 bytes each)
    pub const DAY: u32 = 0xab8ae08b;
    pub const MONTH: u32 = 0x6cf484f4;
    pub const YEAR: u32 = 0x5d7d3f45;
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn ltd_keys(b: &[u8]) -> Option<std::collections::HashMap<u32, usize>> {
    if b.get(..4)? != [4, 3, 2, 1] {
        return None;
    }
    let end = le32(b, 8)? as usize;
    let mut keys = std::collections::HashMap::new();
    let mut at = 0x20;
    while at + 8 <= end.min(b.len()) {
        let (hash, slot) = (le32(b, at)?, le32(b, at + 4)?);
        if hash != 0 {
            keys.insert(hash, slot as usize);
        }
        at += 8;
    }
    Some(keys)
}

/// Fixed-size elements of an array value at `at` (count first).
fn ltd_array(b: &[u8], at: usize, size: usize) -> Vec<&[u8]> {
    let n = le32(b, at).unwrap_or(0) as usize;
    (0..n).map_while(|i| b.get(at + 4 + i * size..at + 4 + (i + 1) * size)).collect()
}

/// A BinaryArray: count, then (size, bytes) each.
fn ltd_binaries(b: &[u8], at: usize) -> Vec<&[u8]> {
    let n = le32(b, at).unwrap_or(0) as usize;
    let mut out = vec![];
    let mut p = at + 4;
    for _ in 0..n {
        let Some(size) = le32(b, p) else { break };
        let Some(e) = b.get(p + 4..p + 4 + size as usize) else { break };
        out.push(e);
        p += 4 + size as usize;
    }
    out
}

fn wstr(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    String::from_utf16_lossy(&u)
}

/// The island's Miis: their CharInfoEx (base64; AzotoTV can't draw it yet:
/// the game numbers hairstyles its own way), name, how to say it, birthday.
pub fn tomodachi_miis(save: &[u8]) -> Vec<Value> {
    let Some(keys) = ltd_keys(save) else { return vec![] };
    let get = |k| keys.get(&k).copied();
    let ints = |k| get(k).map(|at| ltd_array(save, at, 4).iter().map(|x| i32::from_le_bytes([x[0], x[1], x[2], x[3]])).collect::<Vec<_>>()).unwrap_or_default();
    let strs = |k, size| get(k).map(|at| ltd_array(save, at, size).iter().map(|x| wstr(x)).collect::<Vec<_>>()).unwrap_or_default();
    let Some(at) = get(ltd::CHAR_INFO_EX) else { return vec![] };
    let (names, calls) = (strs(ltd::NAME, 64), strs(ltd::HOW_TO_CALL, 128));
    let (day, month, year) = (ints(ltd::DAY), ints(ltd::MONTH), ints(ltd::YEAR));
    ltd_binaries(save, at)
        .iter()
        .enumerate()
        .filter(|(_, ex)| ex.len() >= 0x26 && ex[..16].iter().any(|&b| b != 0))
        .map(|(i, ex)| {
            json!({
                "ex": B64.encode(ex),
                "id": ex[..16].iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "miiName": wstr(&ex[0x10..0x26]),
                "name": names.get(i).cloned().unwrap_or_default(),
                "call": calls.get(i).cloned().unwrap_or_default(),
                "day": day.get(i).copied().unwrap_or(0),
                "month": month.get(i).copied().unwrap_or(0),
                "year": year.get(i).copied().unwrap_or(0),
            })
        })
        .collect()
}

// ---- the sync at game launch -------------------------------------------------
//
// Before a Switch or Wii game, the console's Mii database gets the profile's
// Miis; after it, a database the game changed (a Mii edited or made there)
// goes to the profile's inbox (`players/<id>/mii-inbox/`), which the library
// merges (library.svelte.ts: miijs is needed to read the Miis).
//
// Eden has one database for every profile. The Miis it held before the first
// sync are kept as the shared Miis (`mii/eden-shared.dat`): profiles without a
// Mii library get those, as before, and everyone can import them. What was
// written, and for whom, is kept, so a change no one read back (AzotoTV
// closed during a game) still reaches its owner at the next launch.
// The Wii is one per profile already (its own Dolphin NAND).

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Console {
    Switch,
    Wii,
}

fn mii_dir() -> PathBuf {
    crate::games::data_dir().join("mii")
}

pub fn eden_shared() -> PathBuf {
    mii_dir().join("eden-shared.dat")
}

fn eden_written() -> PathBuf {
    mii_dir().join("eden-written.dat")
}

/// The profile Eden's database was written for ("" = the shared Miis).
fn eden_owner() -> PathBuf {
    mii_dir().join("eden-owner")
}

fn wii_written(player: &Path) -> PathBuf {
    player.join("mii-wii-written.dat")
}

fn inbox(user: &str) -> PathBuf {
    player_dir(user).join("mii-inbox")
}

/// A profile that has a Mii library (it opened the Mii app and added some).
fn has_library(user: &str) -> bool {
    library_path(user).exists()
}

/// A profile's Wii wireless MAC: Nintendo's prefix and 3 bytes from the
/// profile id, the same at every launch (Dolphin would make a random one).
pub fn wii_mac(user: &str) -> [u8; 6] {
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a
    for b in user.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    let b = h.to_be_bytes();
    [0x00, 0x17, 0xab, b[0], b[1], b[2]]
}

/// The console ID a Wii puts in the Miis it makes: a checksum of the MAC's
/// first 3 bytes, then its last 3. Miis with another console's ID are
/// foreign there (blue trousers, not editable in the Mii Channel).
pub fn wii_system_id(mac: [u8; 6]) -> [u8; 4] {
    let sum = mac[..3].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    [sum, mac[3], mac[4], mac[5]]
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("dat.new");
    fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn to_inbox(user: &str, console: Console, db: &[u8]) -> Result<(), String> {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let name = format!("{}-{ms}.dat", if console == Console::Switch { "switch" } else { "wii" });
    write_atomic(&inbox(user).join(name), db)
}

/// The library's encodings for one console, in the library's order.
fn library_entries(user: &str, key: &str) -> Vec<Vec<u8>> {
    load(user)["miis"]
        .as_array()
        .map(|l| l.iter().filter_map(|m| B64.decode(m[key].as_str()?).ok()).collect())
        .unwrap_or_default()
}

/// Eden's database as found: kept as the shared Miis the first time, else
/// handed to whoever it was written for if a game changed it since.
fn eden_take_back(current: &[u8]) -> Result<(), String> {
    let written = fs::read(eden_written()).ok();
    let owner = fs::read_to_string(eden_owner()).unwrap_or_default();
    match written {
        None if !eden_shared().exists() => write_atomic(&eden_shared(), current),
        Some(w) if w != current => match owner.trim() {
            "" => write_atomic(&eden_shared(), current),
            o => to_inbox(o, Console::Switch, current),
        },
        _ => Ok(()),
    }
}

/// Before a game: the console's Miis become this profile's.
pub fn before_game(user: &str, console: Console) -> Result<(), String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    match console {
        Console::Switch => {
            let path = eden_db();
            let current = fs::read(&path).ok();
            if let Some(cur) = &current {
                eden_take_back(cur)?;
            }
            let (owner, db) = if has_library(user) {
                let entries: Vec<_> = library_entries(user, "nx").into_iter().take(NX_SLOTS).collect();
                (user, nx_build(&entries, current.as_deref())?)
            } else {
                match fs::read(eden_shared()).ok().or(current.clone()) {
                    Some(shared) => ("", shared),
                    None => return Ok(()), // Eden has no Miis yet: nothing to do
                }
            };
            if current.as_deref() != Some(&db[..]) {
                write_atomic(&path, &db)?;
            }
            write_atomic(&eden_written(), &db)?;
            fs::write(eden_owner(), owner).map_err(|e| e.to_string())
        }
        Console::Wii => {
            if !has_library(user) {
                return Ok(()); // this Wii's Miis stay as the games left them
            }
            let player = player_dir(user);
            let path = rvl_db(&player);
            let current = fs::read(&path).ok();
            let written = fs::read(wii_written(&player)).ok();
            if let Some(cur) = &current {
                // made in a game, or on this Wii before the first sync
                let has_miis = rvl_entries(cur).is_ok_and(|e| !e.is_empty());
                if written.as_deref() != Some(&cur[..]) && has_miis {
                    to_inbox(user, Console::Wii, cur)?;
                }
            }
            let id = wii_system_id(wii_mac(user));
            let entries: Vec<_> = library_entries(user, "rvl")
                .into_iter()
                .take(RVL_SLOTS)
                .map(|mut e| {
                    e[0x1c..0x20].copy_from_slice(&id);
                    e
                })
                .collect();
            let db = rvl_build(current.as_deref(), &entries)?;
            if current.as_deref() != Some(&db[..]) {
                write_atomic(&path, &db)?;
            }
            write_atomic(&wii_written(&player), &db)
        }
    }
}

/// After a game: a database it changed goes to the inbox. True if one did.
pub fn after_game(user: &str, console: Console) -> Result<bool, String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    let (path, written) = match console {
        Console::Switch => (eden_db(), eden_written()),
        Console::Wii => {
            let player = player_dir(user);
            (rvl_db(&player), wii_written(&player))
        }
    };
    let (Ok(now), Ok(before)) = (fs::read(&path), fs::read(&written)) else { return Ok(false) };
    if now == before {
        return Ok(false);
    }
    match console {
        Console::Switch => eden_take_back(&now)?,
        Console::Wii => to_inbox(user, Console::Wii, &now)?,
    }
    write_atomic(&written, &now)?;
    Ok(true)
}

/// The databases in the profile's inbox, oldest first: `{name, console, miis}`
/// with the Miis raw, base64.
pub fn inbox_list(user: &str) -> Result<Value, String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    let mut names: Vec<String> = fs::read_dir(inbox(user))
        .map(|d| d.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".dat")).collect())
        .unwrap_or_default();
    names.sort_by_key(|n| n.split(['-', '.']).nth(1).and_then(|t| t.parse::<u128>().ok()).unwrap_or(0));
    let mut out = vec![];
    for name in names {
        let Ok(db) = fs::read(inbox(user).join(&name)) else { continue };
        let (console, entries) = if name.starts_with("switch") {
            ("switch", nx_entries(&db))
        } else {
            ("wii", rvl_entries(&db))
        };
        let miis: Vec<String> = entries.unwrap_or_default().iter().map(|e| B64.encode(e)).collect();
        out.push(json!({ "name": name, "console": console, "miis": miis }));
    }
    Ok(Value::Array(out))
}

pub fn inbox_done(user: &str, names: &[String]) -> Result<(), String> {
    check_user(user)?;
    let _g = LOCK.lock().unwrap();
    for n in names {
        if n.contains('/') || n.contains("..") {
            continue;
        }
        let _ = fs::remove_file(inbox(user).join(n));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_is_xmodem() {
        assert_eq!(crc16(b"123456789"), 0x31C3);
    }

    #[test]
    fn blank_wii_database_matches_dolphins() {
        // CRC of the empty RFL_DB.dat Dolphin's Wii Menu created
        let db = rvl_blank();
        assert_eq!(&db[RVL_CRC..RVL_CRC + 2], &[0x9a, 0xff]);
        assert!(rvl_entries(&db).unwrap().is_empty());
    }

    #[test]
    fn wii_round_trip_keeps_hidden_miis() {
        let mut base = rvl_blank();
        base[0x1D10] = 7; // a hidden Mii
        let mii = (0..RVL_ENTRY as u8).collect::<Vec<_>>();
        let db = rvl_build(Some(&base), &[mii.clone()]).unwrap();
        assert_eq!(rvl_entries(&db).unwrap(), vec![mii]);
        assert_eq!(db[0x1D10], 7);
        assert_eq!(&db[RVL_CRC..RVL_CRC + 2], &crc16(&db[..RVL_CRC]).to_be_bytes());
    }

    #[test]
    fn switch_round_trip_fixes_crcs() {
        let mut mii = vec![0u8; NX_ENTRY];
        mii[0] = 0x21;
        let db = nx_build(&[mii.clone(), mii.clone()], None).unwrap();
        let back = nx_entries(&db).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(&back[0][..0x40], &mii[..0x40]);
        assert_eq!(&back[0][0x40..0x42], &crc16(&mii[..0x40]).to_be_bytes());
        assert_eq!(&back[0][0x42..], &EDEN_DEVICE_CRC);
        assert_eq!((db[NX_VERSION], db[NX_COUNT]), (1, 2));
        assert_eq!(&db[NX_LEN - 2..], &crc16(&db[..NX_LEN - 2]).to_be_bytes());
    }

    /// The real databases, when AZOTOTV_MII_SAMPLES points at copies of them.
    #[test]
    fn real_databases_rebuild_identically() {
        let Some(dir) = std::env::var_os("AZOTOTV_MII_SAMPLES").map(PathBuf::from) else { return };
        let nx = fs::read(dir.join("MiiDatabase.dat")).unwrap();
        assert_eq!(nx_build(&nx_entries(&nx).unwrap(), Some(&nx)).unwrap(), nx);
        let rvl = fs::read(dir.join("RFL_DB.dat")).unwrap();
        assert_eq!(rvl_build(None, &rvl_entries(&rvl).unwrap()).unwrap(), rvl);
    }

    /// Two profiles sharing Eden: one without a Mii library (gets the shared
    /// Miis), one with (gets its own); changes come back to the right one.
    #[test]
    fn sync_keeps_each_profiles_miis() {
        let root = crate::test_data_home();
        // SAFETY: only this test reads it
        unsafe { std::env::set_var("AZOTOTV_EDEN_DATA", root.join("eden")) };
        let mii = |n: u8| {
            let mut e = vec![n; NX_ENTRY];
            let crc = crc16(&e[..0x40]);
            e[0x40..0x42].copy_from_slice(&crc.to_be_bytes());
            e[0x42..].copy_from_slice(&EDEN_DEVICE_CRC);
            e
        };
        let shared = nx_build(&[mii(1), mii(2)], None).unwrap();
        write_atomic(&eden_db(), &shared).unwrap();

        // no library: the shared Miis stay, and are kept aside
        before_game("aaa", Console::Switch).unwrap();
        assert_eq!(fs::read(eden_db()).unwrap(), shared);
        assert_eq!(fs::read(eden_shared()).unwrap(), shared);

        // a library: its Miis replace them
        let rvl = vec![9u8; RVL_ENTRY];
        put("bbb", vec![json!({"id": "x", "nx": B64.encode(mii(3)), "rvl": B64.encode(&rvl)})]).unwrap();
        before_game("bbb", Console::Switch).unwrap();
        assert_eq!(nx_entries(&fs::read(eden_db()).unwrap()).unwrap(), vec![mii(3)]);

        // a Mii made in the game reaches bbb's inbox, not the shared Miis
        let edited = nx_build(&[mii(3), mii(4)], None).unwrap();
        write_atomic(&eden_db(), &edited).unwrap();
        assert!(after_game("bbb", Console::Switch).unwrap());
        let inbox = inbox_list("bbb").unwrap();
        assert_eq!(inbox[0]["console"], "switch");
        assert_eq!(inbox[0]["miis"].as_array().unwrap().len(), 2);
        assert!(!after_game("bbb", Console::Switch).unwrap()); // only once

        // back to aaa: the shared Miis again
        before_game("aaa", Console::Switch).unwrap();
        assert_eq!(fs::read(eden_db()).unwrap(), shared);

        // Wii: bbb's Miis with bbb's console ID
        before_game("bbb", Console::Wii).unwrap();
        let wii = rvl_entries(&fs::read(rvl_db(&player_dir("bbb"))).unwrap()).unwrap();
        assert_eq!(wii.len(), 1);
        assert_eq!(&wii[0][0x1c..0x20], &wii_system_id(wii_mac("bbb")));
        // a change nobody read back (AzotoTV closed during the game) still arrives
        let mut db = fs::read(rvl_db(&player_dir("bbb"))).unwrap();
        db[4 + RVL_ENTRY] = 5;
        write_atomic(&rvl_db(&player_dir("bbb")), &db).unwrap();
        before_game("bbb", Console::Wii).unwrap();
        let inbox = inbox_list("bbb").unwrap();
        assert_eq!(inbox.as_array().unwrap().len(), 2);
        assert_eq!(inbox[1]["console"], "wii");
        let names: Vec<String> = inbox.as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap().into()).collect();
        inbox_done("bbb", &names).unwrap();
        assert!(inbox_list("bbb").unwrap().as_array().unwrap().is_empty());
    }

    /// The real game saves, when AZOTOTV_GAME_SAVES points at copies of them
    /// (miitopia.sav, tomodachi-Mii.sav).
    #[test]
    fn reads_game_saves() {
        let Some(dir) = std::env::var_os("AZOTOTV_GAME_SAVES").map(PathBuf::from) else { return };
        let topia = miitopia_miis(&fs::read(dir.join("miitopia.sav")).unwrap());
        let names: Vec<String> = topia.iter().map(|c| wstr(&c[0x10..0x26])).collect();
        eprintln!("miitopia: {names:?}");
        assert!(!names.is_empty() && names.iter().all(|n| !n.is_empty()));
        let tomo = tomodachi_miis(&fs::read(dir.join("tomodachi-Mii.sav")).unwrap());
        eprintln!("tomodachi: {:?}", tomo.iter().map(|m| (m["name"].clone(), m["day"].clone(), m["month"].clone(), m["year"].clone())).collect::<Vec<_>>());
        assert!(!tomo.is_empty());
        for m in &tomo {
            assert!(m["name"].as_str().is_some_and(|n| !n.is_empty()));
            assert!((1..=31).contains(&m["day"].as_i64().unwrap()) && (1..=12).contains(&m["month"].as_i64().unwrap()));
            assert_eq!(m["id"].as_str().map(str::len), Some(32));
        }
    }

    #[test]
    fn wii_console_id_from_the_mac() {
        // 0x00 + 0x17 + 0xab = 0xc2
        assert_eq!(wii_system_id([0x00, 0x17, 0xab, 0x16, 0x22, 0x4e]), [0xc2, 0x16, 0x22, 0x4e]);
        assert_eq!(wii_mac("abc"), wii_mac("abc"));
        assert_ne!(wii_mac("abc"), wii_mac("abd"));
    }
}
