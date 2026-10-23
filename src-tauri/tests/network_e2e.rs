//! Manual end-to-end checks for the two halves of the LAN protocol.
//!
//! These need a real audio endpoint or LAN access, so they are ignored by
//! default. Run them one at a time:
//!
//!   cargo test --test network_e2e -- --ignored --test-threads=1
//!
//! They exercise discovery, subscribe/ack, real Opus encoding or decoding, and
//! audio datagrams over the loopback interface.

use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use locosaonduh_lib::audio::{Encoder, SourceKind};
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
    let port = 53981u16;
    let service = Service::start(dir.clone(), port).expect("service should start");
    wait_for(&service, |snapshot| snapshot.identity.id.len() > 8);
    service
        .start_stream(SourceKind::System)
        .expect("broadcast should start");

    let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
    listener
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let subscribe = Control::Subscribe(Subscribe {
        id: "e2e-listener".to_owned(),
        alias: "E2E listener".to_owned(),
        platform: "linux".to_owned(),
        session: None,
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
            .any(|peer| peer.id == "e2e-sender" && !peer.streams.is_empty())
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

fn test_frames() -> Vec<Vec<u8>> {
    let mut encoder = Encoder::new().unwrap();
    let mut packets = Vec::new();
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
        let mut packet = Vec::new();
        encoder.encode(&pcm, &mut packet).unwrap();
        packets.push(packet);
    }
    packets
}

fn send_test_burst(
    sender: &UdpSocket,
    target: SocketAddr,
    session: u32,
    packets: &[Vec<u8>],
    frames: u32,
) {
    let started = Instant::now();
    let mut previous = Vec::new();
    for seq in 0..frames {
        let due = started + Duration::from_millis(seq as u64 * 10);
        while Instant::now() < due {
            std::hint::spin_loop();
        }
        let current = &packets[seq as usize % packets.len()];
        let datagram = build_audio_packet(
            session,
            seq,
            seq * FRAME_SAMPLES as u32,
            112,
            current,
            &previous,
        );
        sender.send_to(&datagram, target).unwrap();
        previous.clear();
        previous.extend_from_slice(current);
    }
}

#[test]
#[ignore = "requires a real audio output and UDP port 53985"]
fn a_listener_stays_on_standby_and_reconnects() {
    let dir = temp_dir("standby");
    let _ = std::fs::remove_dir_all(&dir);
    let port = 53985u16;
    let service = Service::start(dir.clone(), port).expect("service should start");
    wait_for(&service, |snapshot| snapshot.identity.id.len() > 8);

    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender
        .set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    let sender_port = sender.local_addr().unwrap().port();
    let packets = test_frames();

    let mut first_announce = Announce::new(
        "e2e-standby",
        "Standby sender",
        "linux",
        Some(BroadcastAnnounce {
            session: 100,
            source: "Test tone".to_owned(),
            source_kind: "system".to_owned(),
            started_at_ms: 1,
            listeners: 0,
            codec: CodecAnnounce::default(),
        }),
    );
    first_announce.port = sender_port;
    sender
        .send_to(
            &serde_json::to_vec(&Control::Announce(first_announce.clone())).unwrap(),
            ("127.0.0.1", port),
        )
        .unwrap();
    wait_for(&service, |snapshot| {
        snapshot.peers.iter().any(|peer| peer.id == "e2e-standby")
    });

    let (fake_events, main_events) = mpsc::channel::<&'static str>();
    let (resume_tx, resume_rx) = mpsc::channel::<()>();
    let fake = std::thread::spawn(move || {
        let mut subscriber = None;
        let mut buffer = [0u8; 2_048];
        while subscriber.is_none() {
            let Ok((len, from)) = sender.recv_from(&mut buffer) else {
                continue;
            };
            if let Ok(Control::Subscribe(_)) = serde_json::from_slice::<Control>(&buffer[..len]) {
                subscriber = Some(from);
            }
        }
        let subscriber = subscriber.unwrap();
        let ack = Control::Subscribed(Subscribed {
            session: 100,
            source_id: "e2e-standby".to_owned(),
            source_alias: "Standby sender".to_owned(),
            source: "Test tone".to_owned(),
            source_kind: "system".to_owned(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS as u16,
            frame_ms: 10,
        });
        sender
            .send_to(&serde_json::to_vec(&ack).unwrap(), subscriber)
            .unwrap();
        send_test_burst(&sender, subscriber, 100, &packets, 80);

        sender
            .send_to(
                &serde_json::to_vec(&Control::Ended {
                    session: 100,
                    reason: "test pause".to_owned(),
                })
                .unwrap(),
                subscriber,
            )
            .unwrap();
        let mut paused = first_announce.clone();
        paused.broadcast = None;
        sender
            .send_to(
                &serde_json::to_vec(&Control::Announce(paused)).unwrap(),
                ("127.0.0.1", port),
            )
            .unwrap();
        fake_events.send("paused").unwrap();
        resume_rx.recv_timeout(Duration::from_secs(4)).unwrap();

        let mut resumed = first_announce;
        resumed.broadcast = Some(BroadcastAnnounce {
            session: 101,
            source: "Test tone".to_owned(),
            source_kind: "system".to_owned(),
            started_at_ms: 2,
            listeners: 0,
            codec: CodecAnnounce::default(),
        });
        sender
            .send_to(
                &serde_json::to_vec(&Control::Announce(resumed)).unwrap(),
                ("127.0.0.1", port),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut subscriber = None;
        while Instant::now() < deadline && subscriber.is_none() {
            let Ok((len, from)) = sender.recv_from(&mut buffer) else {
                continue;
            };
            if let Ok(Control::Subscribe(_)) = serde_json::from_slice::<Control>(&buffer[..len]) {
                subscriber = Some(from);
            }
        }
        let subscriber = subscriber.expect("listener should resubscribe automatically");
        let ack = Control::Subscribed(Subscribed {
            session: 101,
            source_id: "e2e-standby".to_owned(),
            source_alias: "Standby sender".to_owned(),
            source: "Test tone".to_owned(),
            source_kind: "system".to_owned(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS as u16,
            frame_ms: 10,
        });
        sender
            .send_to(&serde_json::to_vec(&ack).unwrap(), subscriber)
            .unwrap();
        send_test_burst(&sender, subscriber, 101, &packets, 80);
    });

    service
        .subscribe("e2e-standby")
        .expect("initial subscribe should work");
    wait_for(&service, |snapshot| {
        snapshot
            .listening
            .iter()
            .any(|stream| stream.state == "live")
    });
    assert_eq!(
        main_events.recv_timeout(Duration::from_secs(4)).unwrap(),
        "paused"
    );
    wait_for(&service, |snapshot| {
        snapshot
            .listening
            .iter()
            .any(|stream| stream.state == "standby")
    });
    resume_tx.send(()).unwrap();
    wait_for(&service, |snapshot| {
        snapshot
            .listening
            .iter()
            .any(|stream| stream.state == "live")
    });
    service.unsubscribe("e2e-standby").unwrap();
    fake.join().unwrap();
    service.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

fn introduce(from: &Service, from_port: u16, to_port: u16) {
    let snapshot = from.snapshot();
    let streams = snapshot
        .broadcasts
        .iter()
        .map(|stream| BroadcastAnnounce {
            session: stream.session,
            source: stream.source.clone(),
            source_kind: stream.kind.clone(),
            started_at_ms: stream.started_at_ms,
            listeners: 0,
            codec: CodecAnnounce::default(),
        })
        .collect();
    let mut announce = Announce::new(
        snapshot.identity.id.clone(),
        snapshot.identity.alias.clone(),
        "windows",
        None,
    )
    .with_streams(streams);
    announce.port = from_port;
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .send_to(
            &serde_json::to_vec(&Control::Announce(announce)).unwrap(),
            ("127.0.0.1", to_port),
        )
        .unwrap();
}

#[test]
#[ignore = "requires real audio endpoints and UDP ports 53986-53987"]
fn two_devices_play_each_other() {
    let (port_a, port_b) = (53986u16, 53987u16);
    let (dir_a, dir_b) = (temp_dir("mutual-a"), temp_dir("mutual-b"));
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
    let a = Service::start(dir_a.clone(), port_a).expect("A should start");
    let b = Service::start(dir_b.clone(), port_b).expect("B should start");
    for service in [&a, &b] {
        wait_for(service, |snapshot| snapshot.identity.id.len() > 8);
        service
            .start_stream(SourceKind::System)
            .expect("system sound should start");
    }
    let id_a = a.snapshot().identity.id;
    let id_b = b.snapshot().identity.id;
    introduce(&a, port_a, port_b);
    introduce(&b, port_b, port_a);
    wait_for(&a, |snapshot| {
        snapshot.peers.iter().any(|peer| peer.id == id_b)
    });
    wait_for(&b, |snapshot| {
        snapshot.peers.iter().any(|peer| peer.id == id_a)
    });

    a.subscribe(format!("{id_b}#system"))
        .expect("A should listen to B");
    b.subscribe(format!("{id_a}#system"))
        .expect("B should listen to A");

    for (service, other) in [(&a, &id_b), (&b, &id_a)] {
        let snapshot = wait_for(service, |snapshot| {
            snapshot
                .listening
                .iter()
                .any(|stream| &stream.sender_id == other && stream.state != "standby")
                && snapshot
                    .broadcasts
                    .iter()
                    .any(|stream| stream.listeners.len() == 1)
        });
        let own = &snapshot.broadcasts[0];
        if cfg!(target_os = "windows") {
            assert!(own.excludes_self, "capture must leave this app out");
        }
    }

    a.unsubscribe(id_b).unwrap();
    b.unsubscribe(id_a).unwrap();
    a.shutdown();
    b.shutdown();
    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}

#[test]
#[ignore = "requires VB-CABLE, a microphone and UDP ports 53988-53989"]
fn a_forwarded_microphone_lands_on_the_virtual_cable() {
    let (port_a, port_b) = (53988u16, 53989u16);
    let (dir_a, dir_b) = (temp_dir("virtual-a"), temp_dir("virtual-b"));
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
    let a = Service::start(dir_a.clone(), port_a).expect("A should start");
    let b = Service::start(dir_b.clone(), port_b).expect("B should start");
    wait_for(&a, |snapshot| snapshot.virtual_mic.state == "ready");
    a.set_virtual_mic(true)
        .expect("virtual microphone should enable");
    wait_for(&b, |snapshot| !snapshot.sources.is_empty());
    b.start_stream(SourceKind::Microphone)
        .expect("microphone should start");
    let id_a = a.snapshot().identity.id;
    let id_b = b.snapshot().identity.id;
    introduce(&b, port_b, port_a);
    introduce(&a, port_a, port_b);
    wait_for(&a, |snapshot| {
        snapshot.peers.iter().any(|peer| peer.id == id_b)
    });

    a.subscribe(format!("{id_b}#microphone"))
        .expect("A should listen to B's microphone");
    let snapshot = wait_for(&a, |snapshot| {
        snapshot
            .listening
            .iter()
            .any(|stream| stream.kind == "microphone" && stream.to_virtual)
    });
    let stream = &snapshot.listening[0];
    assert!(
        stream.output.contains("CABLE"),
        "routed to {}",
        stream.output
    );
    assert!(
        !snapshot
            .sources
            .iter()
            .any(|source| source.name.starts_with("CABLE Output")),
        "the virtual cable must not be offered as a microphone"
    );

    a.unsubscribe(format!("{id_b}#microphone")).unwrap();
    let _ = id_a;
    a.shutdown();
    b.shutdown();
    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}
