//! Manual end-to-end checks for the two halves of the LAN protocol.
//!
//! These need a real audio endpoint or LAN access, so they are ignored by
//! default. Run them one at a time:
//!
//!   cargo test --test network_e2e -- --ignored --test-threads=1
//!
//! They exercise discovery, subscribe/ack, real Opus encoding or decoding, and
//! audio datagrams over the loopback interface.

use std::net::UdpSocket;
use std::time::{Duration, Instant};

use locosaonduh_lib::audio::{Encoder, SourceKind, list_sources};
use locosaonduh_lib::protocol::{
    Announce, BroadcastAnnounce, CHANNELS, CodecAnnounce, Control, FRAME_SAMPLES, SAMPLE_RATE,
    Subscribe, Subscribed, build_audio_packet, is_audio_datagram, parse_audio_packet,
};
use locosaonduh_lib::service::Service;
use locosaonduh_lib::state::Snapshot;

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("locosaonduh-{name}-{}", std::process::id()))
}

fn wait_for<F>(service: &Service, mut predicate: F) -> Snapshot
where
    F: FnMut(&Snapshot) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = service.snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        if Instant::now() > deadline {
            panic!("condition not reached: {snapshot:#?}");
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[test]
#[ignore = "requires real audio endpoints and UDP port 53410"]
fn a_udp_listener_hears_the_sender() {
    let dir = temp_dir("sender");
    let _ = std::fs::remove_dir_all(&dir);
    let source = list_sources()
        .into_iter()
        .find(|source| source.kind == SourceKind::System)
        .expect("a system output is required");
    let port = 53981u16;
    let service = Service::start(dir.clone(), port).expect("service should start");
    wait_for(&service, |snapshot| {
        snapshot.sources.iter().any(|item| item.id == source.id)
    });
    service
        .start_broadcast(source.id.clone())
        .expect("broadcast should start");

    let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
    listener
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let subscribe = Control::Subscribe(Subscribe {
        id: "e2e-listener".to_owned(),
        alias: "E2E listener".to_owned(),
        platform: "linux".to_owned(),
    });
    listener
        .send_to(
            &serde_json::to_vec(&subscribe).unwrap(),
            ("127.0.0.1", port),
        )
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(8);
    let mut received_audio = false;
    let mut acknowledged = false;
    while Instant::now() < deadline && !received_audio {
        let mut buffer = [0u8; 2_048];
        let Ok((len, _)) = listener.recv_from(&mut buffer) else {
            continue;
        };
        let payload = &buffer[..len];
        if is_audio_datagram(payload) {
            let packet = parse_audio_packet(payload).expect("audio header should parse");
            assert_ne!(packet.session, 0);
            assert!(!packet.current.is_empty());
            received_audio = true;
        } else if let Ok(Control::Subscribed(ack)) = serde_json::from_slice::<Control>(payload) {
            assert_eq!(ack.sample_rate, SAMPLE_RATE);
            acknowledged = true;
        }
    }
    assert!(acknowledged, "sender did not acknowledge the subscription");
    assert!(received_audio, "sender did not deliver audio datagrams");

    service.stop_broadcast().expect("broadcast should stop");
    service.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "requires real audio endpoints and UDP port 53410"]
fn a_subscribed_receiver_decodes_audio() {
    let dir = temp_dir("receiver");
    let _ = std::fs::remove_dir_all(&dir);
    let port = 53982u16;
    let service = Service::start(dir.clone(), port).expect("service should start");
    wait_for(&service, |snapshot| snapshot.identity.id.len() > 8);

    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender
        .set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    let sender_port = sender.local_addr().unwrap().port();
    let announce = Control::Announce({
        let mut announce = Announce::new(
            "e2e-sender",
            "E2E sender",
            "linux",
            Some(BroadcastAnnounce {
                session: 42,
                source: "Test tone".to_owned(),
                source_kind: "system".to_owned(),
                started_at_ms: 1,
                listeners: 0,
                codec: CodecAnnounce::default(),
            }),
        );
        announce.port = sender_port;
        announce
    });
    sender
        .send_to(&serde_json::to_vec(&announce).unwrap(), ("127.0.0.1", port))
        .unwrap();
    wait_for(&service, |snapshot| {
        snapshot
            .peers
            .iter()
            .any(|peer| peer.id == "e2e-sender" && peer.is_broadcasting)
    });

    let fake = std::thread::spawn(move || {
        let mut subscribe_from = None;
        let mut buffer = [0u8; 2_048];
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            let Ok((len, from)) = sender.recv_from(&mut buffer) else {
                continue;
            };
            if let Ok(Control::Subscribe(_)) = serde_json::from_slice::<Control>(&buffer[..len]) {
                subscribe_from = Some(from);
                break;
            }
        }
        let subscriber = subscribe_from.expect("receiver never sent Subscribe");
        let ack = Control::Subscribed(Subscribed {
            session: 42,
            source_id: "e2e-sender".to_owned(),
            source_alias: "E2E sender".to_owned(),
            source: "Test tone".to_owned(),
            source_kind: "system".to_owned(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS as u16,
            frame_ms: 10,
        });
        sender
            .send_to(&serde_json::to_vec(&ack).unwrap(), subscriber)
            .unwrap();

        // A handful of real Opus packets is enough: the receiver only needs
        // valid frames. Cycling them keeps the debug-build encoder out of the
        // timing loop, so the ten-millisecond wire pacing is what gets tested.
        let mut encoder = Encoder::new().unwrap();
        let mut encoded = Vec::new();
        for packet_index in 0..8u32 {
            let mut pcm = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
            for (index, frame) in pcm.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
                let phase = ((index + packet_index as usize * FRAME_SAMPLES) as f32
                    * 440.0
                    * std::f32::consts::TAU
                    / SAMPLE_RATE as f32)
                    .sin()
                    * 0.005;
                frame[0] = phase;
                frame[1] = phase;
            }
            let mut current = Vec::new();
            encoder.encode(&pcm, &mut current).unwrap();
            encoded.push(current);
        }
        let mut previous = Vec::new();
        let started = Instant::now();
        for seq in 0..240u32 {
            let due = started + Duration::from_millis(seq as u64 * 10);
            while Instant::now() < due {
                std::hint::spin_loop();
            }
            let current = &encoded[seq as usize % encoded.len()];
            let packet =
                build_audio_packet(42, seq, seq * FRAME_SAMPLES as u32, 112, current, &previous);
            sender.send_to(&packet, subscriber).unwrap();
            previous.clear();
            previous.extend_from_slice(current);
        }
    });

    service
        .subscribe("e2e-sender")
        .expect("receiver should subscribe");
    let snapshot = wait_for(&service, |snapshot| {
        snapshot
            .listening
            .iter()
            .any(|stream| stream.state == "live" && stream.buffer_ms >= 20)
    });
    assert!(
        snapshot.listening[0].loss_pct < 1.0,
        "playout lost {:?}",
        snapshot.listening[0]
    );
    service.unsubscribe("e2e-sender").unwrap();
    fake.join().unwrap();
    service.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "requires UDP port 53984 and a local network"]
fn a_unicast_probe_teaches_both_sides() {
    let dir = temp_dir("probe");
    let _ = std::fs::remove_dir_all(&dir);
    let port = 53984u16;
    let service = Service::start(dir.clone(), port).expect("service should start");
    wait_for(&service, |snapshot| snapshot.identity.id.len() > 8);

    let fake = UdpSocket::bind("127.0.0.1:0").unwrap();
    fake.set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    let fake_port = fake.local_addr().unwrap().port();
    let mut announce = Announce::new("probe-peer", "Probe peer", "linux", None);
    announce.port = fake_port;
    let probe = Control::Probe {
        id: "probe-peer".to_owned(),
        announce: Some(announce),
    };
    fake.send_to(&serde_json::to_vec(&probe).unwrap(), ("127.0.0.1", port))
        .unwrap();

    wait_for(&service, |snapshot| {
        snapshot.peers.iter().any(|peer| peer.id == "probe-peer")
    });

    let mut response = None;
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        let mut buffer = [0u8; 2_048];
        let Ok((len, _)) = fake.recv_from(&mut buffer) else {
            continue;
        };
        if let Ok(Control::Announce(announce)) = serde_json::from_slice::<Control>(&buffer[..len]) {
            response = Some(announce);
            break;
        }
    }
    let response = response.expect("the probe should receive an announcement");
    assert!(response.is_compatible());
    assert_eq!(response.port, port);

    service.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}
