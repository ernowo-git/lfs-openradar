//! InSim version 9/10 framing and the packets needed for a local radar.
use crate::radar::Pose;
use std::collections::BTreeMap;

pub const ISS_GAME: u16 = 1;
pub const ISS_REPLAY: u16 = 2;
pub const ISS_PAUSED: u16 = 4;
pub const ISS_SHIFTU: u16 = 8;
pub const ISS_FRONT_END: u16 = 256;
pub const ISS_MULTI: u16 = 512;

#[derive(Clone, Debug)]
pub struct Car {
    pub node: u16,
    pub lap: u16,
    pub position: u8,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackInfo {
    pub track: String,
    pub nodes: u16,
    pub finish: u16,
    pub split1: u16,
    pub timing: u8,
    pub race_laps: u8,
}
impl TrackInfo {
    pub fn supports_gaps(&self) -> bool {
        self.nodes > 1 && self.finish < self.nodes && self.timing & 0xc0 == 0x40
    }
}
#[derive(Clone, Debug)]
pub enum Packet {
    Version {
        version: String,
        protocol: u8,
    },
    Tiny(u8),
    State(State),
    Player(Player),
    /// Reply to a roster request, rather than a car joining or re-entering.
    PlayerSnapshot(Player),
    ConnectionLeft(u8),
    Pit(u8),
    Leave(u8),
    Reset(u8),
    Takeover(u8),
    Camera(u8),
    Session,
    RaceStart {
        info: TrackInfo,
        requested: bool,
    },
    Lap {
        plid: u8,
        time_ms: u32,
        penalty: u8,
    },
    Split {
        plid: u8,
        time_ms: u32,
        split: u8,
        penalty: u8,
    },
    InvalidLap(u8),
    LayoutChanged,
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
    // LOCAL | MCI | HLV: receive track-limit/wall/pit-speed violations.
    p[6..8].copy_from_slice(&0x124_u16.to_le_bytes());
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
        17 => {
            exact(28)?;
            Packet::RaceStart {
                requested: p[2] != 0,
                info: TrackInfo {
                    track: text(&p[8..14]),
                    nodes: u16_at(p, 18),
                    finish: u16_at(p, 20),
                    split1: u16_at(p, 22),
                    timing: p[7],
                    race_laps: p[4],
                },
            }
        }
        19 => {
            exact(8)?;
            Packet::ConnectionLeft(p[3])
        }
        21 => {
            exact(76)?;
            if p[73] == 0 {
                return Ok(Packet::Other);
            } // Join request, not a racer.
            let player = Player {
                plid: p[3],
                ucid: p[4],
                kind: p[5],
                name: plain_driver_name(&text(&p[8..32])),
                model: text(&p[40..44]),
                in_garage: false,
            };
            if p[2] != 0 {
                Packet::PlayerSnapshot(player)
            } else {
                Packet::Player(player)
            }
        }
        22 => {
            exact(4)?;
            Packet::Pit(p[3])
        }
        23 => {
            exact(4)?;
            Packet::Leave(p[3])
        }
        24 => {
            exact(20)?;
            Packet::Lap {
                plid: p[3],
                time_ms: u32_at(p, 4),
                penalty: p[17],
            }
        }
        25 => {
            exact(16)?;
            Packet::Split {
                plid: p[3],
                time_ms: u32_at(p, 4),
                split: p[12],
                penalty: p[13],
            }
        }
        26 => {
            exact(24)?;
            Packet::InvalidLap(p[3])
        }
        30 => {
            exact(8)?;
            if p[5] != 0 {
                Packet::InvalidLap(p[3])
            } else {
                Packet::Other
            }
        }
        43 => {
            exact(40)?;
            Packet::LayoutChanged
        }
        52 => {
            // HLV is 16 bytes in v9 and 20 bytes in v10.
            if !matches!(p.len(), 16 | 20) {
                return Err("HLV requires 16 or 20 bytes".into());
            }
            Packet::InvalidLap(p[3])
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
                    node: u16_at(c, 0),
                    lap: u16_at(c, 2),
                    position: c[5],
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
/// Driver labels use the overlay's colors rather than LFS's inline color codes.
fn plain_driver_name(name: &str) -> String {
    let mut chars = name.chars().peekable();
    let mut result = String::with_capacity(name.len());
    while let Some(character) = chars.next() {
        if character == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            result.push(character);
        }
    }
    result
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
