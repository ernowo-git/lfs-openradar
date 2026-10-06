mod support;
use lfs_openradar::lfs::{
    insim::{self, Framer, MciAssembler, Packet},
    outsim,
};
use support::*;

#[test]
fn tcp_fragments_and_coalesced_packets() {
    let mut framer = Framer::default();
    let first = mci(&[1, 2], true, true);
    assert!(framer.push(&first[..3]).unwrap().is_empty());
    let tail = [first[3..].to_vec(), insim::tiny(0, 0).to_vec()].concat();
    let packets = framer.push(&tail).unwrap();
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0], first);
    assert_eq!(packets[1], [1, 3, 0, 0]);
    assert!(framer.push(&[0]).is_err());
}
#[test]
fn mci_units_and_signed_coordinates() {
    let mut bytes = mci(&[7], true, true);
    bytes[4..6].copy_from_slice(&345_u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&12_u16.to_le_bytes());
    bytes[9] = 4;
    let Packet::Mci(cars) = insim::decode(&bytes).unwrap() else {
        panic!()
    };
    assert_eq!(cars[0].pose.x, -1.0);
    assert_eq!((cars[0].node, cars[0].lap, cars[0].position), (345, 12, 4));
    assert_eq!(cars[0].pose.y, 2.0);
    assert_eq!(cars[0].speed_mps, 100.0);
    assert!((cars[0].pose.heading - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
}
#[test]
fn race_start_track_metadata_is_validated_and_preserved() {
    let mut bytes = packet(17, 28);
    bytes[2] = 1;
    bytes[4] = 10;
    bytes[7] = 0x41;
    bytes[8..12].copy_from_slice(b"BL1R");
    bytes[18..20].copy_from_slice(&800_u16.to_le_bytes());
    bytes[20..22].copy_from_slice(&100_u16.to_le_bytes());
    let Packet::RaceStart { info, requested } = insim::decode(&bytes).unwrap() else {
        panic!()
    };
    assert!(requested);
    assert!(info.supports_gaps());
    assert_eq!(info.track, "BL1R");
    assert_eq!((info.nodes, info.finish, info.race_laps), (800, 100, 10));
    assert!(insim::decode(&packet(17, 4)).is_err());
}
#[test]
fn driver_names_ignore_color_codes_and_preserve_text() {
    for (name, expected) in [
        ("^3Mas ^7Verstappen", "Mas Verstappen"),
        ("^0^1^2^3^4^5^6^7^8^9Max", "Max"),
        ("^2José ^7林", "José 林"),
        ("Max^", "Max^"),
        ("^Max ^x Driver", "^Max ^x Driver"),
        ("Plain Driver", "Plain Driver"),
    ] {
        let mut bytes = packet(21, 76);
        bytes[3] = 7;
        bytes[73] = 1;
        bytes[8..8 + name.len()].copy_from_slice(name.as_bytes());
        let Packet::Player(player) = insim::decode(&bytes).unwrap() else {
            panic!()
        };
        assert_eq!(player.name, expected);
    }
}
#[test]
fn sets_of_17_and_48_cars_are_atomic() {
    for count in [17, 48] {
        let mut assembly = MciAssembler::default();
        let ids: Vec<u8> = (1..=count).collect();
        for (i, chunk) in ids.chunks(16).enumerate() {
            let last = (i + 1) * 16 >= count as usize;
            let Packet::Mci(cars) = insim::decode(&mci(chunk, i == 0, last)).unwrap() else {
                panic!()
            };
            let result = assembly.push(cars).unwrap();
            if last {
                assert_eq!(result.unwrap().len(), count as usize);
            } else {
                assert!(result.is_none());
            }
        }
    }
}
#[test]
fn incomplete_duplicate_and_malformed_mci() {
    let mut assembly = MciAssembler::default();
    let Packet::Mci(first) = insim::decode(&mci(&[1], true, false)).unwrap() else {
        panic!()
    };
    assert!(assembly.push(first).unwrap().is_none());
    let Packet::Mci(duplicate) = insim::decode(&mci(&[1], false, true)).unwrap() else {
        panic!()
    };
    assert!(assembly.push(duplicate).is_err());
    let mut bad = mci(&[1], true, true);
    bad[3] = 16;
    assert!(insim::decode(&bad).is_err());
    for size in 0..32 {
        assert!(insim::decode(&vec![0; size]).is_err());
    }
}
#[test]
fn full_and_legacy_outsim_layouts() {
    let full = full_outsim(1250, -12.0, 4.0, 1.2);
    let sample = outsim::decode(&full, 0x1ff, Some(24601)).unwrap();
    assert_eq!(sample.time_ms, 1250);
    assert_eq!(sample.pose.x, -12.0);
    assert_eq!(sample.pose.y, 4.0);
    assert!((sample.pose.heading - 1.2).abs() < 1e-6);
    let mut legacy = vec![0; 64];
    legacy[..4].copy_from_slice(&1250_u32.to_le_bytes());
    legacy[4..64].copy_from_slice(&full[12..72]);
    assert_eq!(outsim::decode(&legacy, 0, None).unwrap().pose.x, -12.0);
    legacy.extend_from_slice(&24601_i32.to_le_bytes());
    assert_eq!(
        outsim::decode(&legacy, 0, Some(24601)).unwrap().time_ms,
        1250
    );
}
#[test]
fn outsim_rejects_layout_id_nan_and_outgauge() {
    let mut p = full_outsim(0, 0.0, 0.0, 0.0);
    assert!(outsim::decode(&p, 0x1ff, Some(7)).is_err());
    p[24..28].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(outsim::decode(&p, 0x1ff, Some(24601)).is_err());
    assert!(outsim::decode(&[0; 96], 0x1ff, Some(24601)).is_err());
    for length in 0..280 {
        assert!(outsim::decode(&p[..length], 0x1ff, Some(24601)).is_err());
    }
}
#[test]
fn initialization_keeps_mci_on_tcp() {
    let p = insim::init(20, "").unwrap();
    assert_eq!(&p[..12], &[11, 1, 1, 0, 0, 0, 36, 1, 9, 0, 20, 0]);
    assert!(insim::init(20, &"a".repeat(16)).is_err());
}
