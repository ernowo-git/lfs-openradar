//! InSim version 9/10 framing and the packets needed for a local radar.
use crate::radar::Pose;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Car {
    pub plid: u8,
    pub info: u8,
    pub pose: Pose,
    pub speed_mps: f64,
    pub direction: f64,
}
#[derive(Clone, Debug)]
pub struct Player {
    pub plid: u8,
    pub ucid: u8,
    pub kind: u8,
    pub name: String,
    pub model: String,
    pub in_garage: bool,
}
impl Player {
    pub fn local_human(&self) -> bool {
        self.kind & 6 == 0 && !self.in_garage
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub flags: u16,
    pub camera: u8,
    pub viewed: u8,
    pub track: String,
}
#[derive(Clone, Debug)]
pub enum Packet {
    Version { version: String, protocol: u8 },
    Tiny(u8),
    State(State),
    Player(Player),
    ConnectionLeft(u8),
    Pit(u8),
    Leave(u8),
    Reset(u8),
    Takeover(u8),
    Camera(u8),
    Session,
    Mci(Vec<Car>),
    Other,
}
#[derive(Default)]
pub struct Framer {
    buffer: Vec<u8>,
}
impl Framer {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        self.buffer.extend_from_slice(bytes);
        let mut result = Vec::new();
        let mut consumed = 0;
        while consumed < self.buffer.len() {
            let size = usize::from(self.buffer[consumed]) * 4;
            if size < 4 {
                return Err("Invalid InSim size byte".into());
            }
            if self.buffer.len() - consumed < size {
                break;
            }
            result.push(self.buffer[consumed..consumed + size].to_vec());
            consumed += size;
        }
        self.buffer.drain(..consumed);
        Ok(result)
    }
}
pub fn tiny(subtype: u8, request: u8) -> [u8; 4] {
    [1, 3, request, subtype]
}
pub fn init(interval: u16, password: &str) -> Result<[u8; 44], String> {
    if password.len() > 15 || !password.is_ascii() || password.contains('\0') {
        return Err("InSim password must be at most 15 ASCII characters".into());
    }
    let mut p = [0; 44];
    p[0..4].copy_from_slice(&[11, 1, 1, 0]);
    // UDPPort=0 keeps MCI on TCP. OutSim uses its cfg.txt destination.
    p[6..8].copy_from_slice(&0x24_u16.to_le_bytes());
    p[8] = 9; // LFS 0.7A+ compatibility; only shared v9/v10 packets are used.
    p[10..12].copy_from_slice(&interval.to_le_bytes());
    p[12..12 + password.len()].copy_from_slice(password.as_bytes());
    p[28..41].copy_from_slice(b"lfs-openradar");
    Ok(p)
}
pub fn decode(p: &[u8]) -> Result<Packet, String> {
    if p.len() < 4 || usize::from(p[0]) * 4 != p.len() {
        return Err("InSim packet length does not match its header".into());
    }
    let exact = |n: usize| -> Result<(), String> {
        if p.len() == n {
            Ok(())
        } else {
            Err(format!("Packet {} requires {n} bytes", p[1]))
        }
    };
    Ok(match p[1] {
        2 => {
            exact(20)?;
            Packet::Version {
                version: text(&p[4..12]),
                protocol: p[18],
            }
        }
        3 => {
            exact(4)?;
            Packet::Tiny(p[3])
        }
        5 => {
            exact(28)?;
            Packet::State(State {
                flags: u16_at(p, 8),
                camera: p[10],
                viewed: p[11],
                track: text(&p[20..26]),
            })
        }
        17 => Packet::Session,
        19 => {
            exact(8)?;
            Packet::ConnectionLeft(p[3])
        }
        21 => {
            exact(76)?;
            if p[73] == 0 {
                return Ok(Packet::Other);
            } // Join request, not a racer.
            Packet::Player(Player {
                plid: p[3],
                ucid: p[4],
                kind: p[5],
                name: text(&p[8..32]),
                model: text(&p[40..44]),
                in_garage: false,
            })
        }
        22 => {
            exact(4)?;
            Packet::Pit(p[3])
        }
        23 => {
            exact(4)?;
            Packet::Leave(p[3])
        }
        29 => {
            exact(8)?;
            Packet::Camera(p[3])
        }
        31 => {
            exact(8)?;
            Packet::Takeover(p[3])
        }
        38 => {
            let count = usize::from(p[3]);
            if count > 16 || p.len() != 4 + 28 * count {
                return Err("Invalid MCI count or entry length".into());
            }
            let mut cars = Vec::with_capacity(count);
            for c in p[4..].as_chunks::<28>().0 {
                if c[4] == 0 {
                    return Err("MCI contains reserved PLID zero".into());
                }
                cars.push(Car {
                    plid: c[4],
                    info: c[6],
                    pose: Pose {
                        x: f64::from(i32_at(c, 8)) / 65536.0,
                        y: f64::from(i32_at(c, 12)) / 65536.0,
                        z: f64::from(i32_at(c, 16)) / 65536.0,
                        heading: angle(u16_at(c, 24)),
                    },
                    speed_mps: f64::from(u16_at(c, 20)) * 100.0 / 32768.0,
                    direction: angle(u16_at(c, 22)),
                });
            }
            Packet::Mci(cars)
        }
        41 => {
            exact(4)?;
            Packet::Reset(p[3])
        }
        _ => Packet::Other,
    })
}
fn angle(raw: u16) -> f64 {
    f64::from(raw) * std::f64::consts::TAU / 65536.0
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.split(|b| *b == 0).next().unwrap_or_default()).into_owned()
}
pub(super) fn u16_at(p: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(p[offset..offset + 2].try_into().unwrap())
}
pub(super) fn u32_at(p: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(p[offset..offset + 4].try_into().unwrap())
}
pub(super) fn i32_at(p: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(p[offset..offset + 4].try_into().unwrap())
}

/// Publish a snapshot only after an ordered FIRST..LAST sequence.
#[derive(Default)]
pub struct MciAssembler {
    pending: BTreeMap<u8, Car>,
    active: bool,
}
impl MciAssembler {
    pub fn clear(&mut self) {
        self.pending.clear();
        self.active = false;
    }
    pub fn push(&mut self, cars: Vec<Car>) -> Result<Option<Vec<Car>>, String> {
        if cars.is_empty() {
            self.clear();
            return Ok(Some(Vec::new()));
        }
        let count = cars.len();
        for (i, car) in cars.into_iter().enumerate() {
            if car.info & 64 != 0 {
                if i != 0 {
                    self.clear();
                    return Err("FIRST flag inside MCI packet".into());
                }
                self.clear();
                self.active = true;
            }
            if !self.active {
                return Ok(None);
            }
            let last = car.info & 128 != 0;
            if self.pending.insert(car.plid, car).is_some() || self.pending.len() > 48 {
                self.clear();
                return Err("Duplicate car or too many cars in MCI set".into());
            }
            if last {
                if i + 1 != count {
                    self.clear();
                    return Err("LAST flag before end of MCI packet".into());
                }
                self.active = false;
                return Ok(Some(
                    std::mem::take(&mut self.pending).into_values().collect(),
                ));
            }
        }
        Ok(None)
    }
}
