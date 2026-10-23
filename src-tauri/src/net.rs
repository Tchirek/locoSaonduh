use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use crate::protocol::{DEFAULT_PORT, MULTICAST_ADDR};

/// A directly connected IPv4 network. Discovery is sent from this exact source
/// address so a VPN or virtual adapter cannot swallow the broadcast.
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub broadcast: Ipv4Addr,
}

const MAX_SCAN_HOSTS: usize = 1_024;
const EXPEDITED_FORWARDING: u32 = 46 << 2;

pub fn ipv4_interfaces() -> Vec<NetworkInterface> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut result = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(address)
                if !address.ip.is_loopback() && !address.ip.is_link_local() =>
            {
                let derived = Ipv4Addr::from(u32::from(address.ip) | !u32::from(address.netmask));
                Some(NetworkInterface {
                    name: interface.name,
                    ip: address.ip,
                    netmask: address.netmask,
                    broadcast: address.broadcast.unwrap_or(derived),
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|interface| interface.ip);
    result.dedup_by_key(|interface| interface.ip);
    result
}

pub fn bind_discovery_socket(port: u16) -> Result<Arc<UdpSocket>> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .context("could not create discovery socket")?;
    socket
        .set_reuse_address(true)
        .context("could not reuse discovery address")?;
    socket
        .set_broadcast(true)
        .context("could not enable LAN broadcast")?;
    socket
        .set_multicast_loop_v4(true)
        .context("could not enable multicast loop")?;
    socket
        .set_multicast_ttl_v4(1)
        .context("could not set multicast TTL")?;
    // A subnet sweep is sent in one short burst; a small default send buffer
    // can turn that into silent EWOULDBLOCK drops.
    if let Err(error) = socket.set_send_buffer_size(1 << 20) {
        log::debug!("could not enlarge discovery send buffer: {error}");
    }
    if let Err(error) = socket.set_recv_buffer_size(1 << 20) {
        log::debug!("could not enlarge discovery receive buffer: {error}");
    }
    // 声の荷に 急ぎの札を 括り付け
    if let Err(error) = socket.set_tos_v4(EXPEDITED_FORWARDING) {
        log::debug!("could not mark audio as expedited: {error}");
    }
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())
        .with_context(|| format!("UDP port {port} is already in use"))?;
    // 門番は 百ミリごとに 振り返る
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .context("could not bound discovery socket reads")?;
    let socket: UdpSocket = socket.into();

    // Join on every live interface, not just the route-table default. A VPN
    // adapter often owns the default route while the actual LAN lives on Wi-Fi
    // or Ethernet.
    let interfaces = ipv4_interfaces();
    let mut joined = 0usize;
    for interface in &interfaces {
        match socket.join_multicast_v4(&MULTICAST_ADDR, &interface.ip) {
            Ok(()) => joined += 1,
            Err(error) => log::debug!(
                "multicast join on {} ({}) failed: {error}",
                interface.name,
                interface.ip
            ),
        }
    }
    if joined == 0
        && let Err(error) = socket.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED)
    {
        log::warn!("multicast join failed; broadcast and subnet scan remain active: {error}");
    }
    Ok(Arc::new(socket))
}

/// Send on each directly connected interface. A socket bound to the interface
/// address forces both the source address and the outgoing interface; binding
/// only the destination broadcast is not enough when a VPN owns the default
/// route.
pub fn send_discovery(payload: &[u8], port: u16) -> usize {
    let interfaces = ipv4_interfaces();
    let mut sent = 0usize;
    if interfaces.is_empty() {
        if let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
            let _ = socket.set_broadcast(true);
            sent += usize::from(socket.send_to(payload, (MULTICAST_ADDR, port)).is_ok());
            sent += usize::from(socket.send_to(payload, (Ipv4Addr::BROADCAST, port)).is_ok());
        }
        return sent;
    }

    for interface in &interfaces {
        let Ok(sender) = interface_sender(interface.ip) else {
            continue;
        };
        for target in [MULTICAST_ADDR, Ipv4Addr::BROADCAST, interface.broadcast] {
            if target == interface.ip {
                continue;
            }
            if sender.send_to(payload, (target, port)).is_ok() {
                sent += 1;
            }
        }
    }
    if sent == 0 {
        log::debug!("no discovery datagrams could be sent on a local interface");
    }
    sent
}

fn interface_sender(ip: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_broadcast(true)?;
    socket.set_multicast_loop_v4(true)?;
    socket.set_multicast_ttl_v4(1)?;
    socket.set_multicast_if_v4(&ip)?;
    socket.bind(&SocketAddr::from((ip, 0)).into())?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

/// Unicast probe every usable host in each directly connected /24 (or in a
/// larger subnet when it is small enough to remain polite). The probes travel
/// from the main listening socket, so an older peer that only understands the
/// original probe can still send its answer to the right port.
pub fn scan_discovery(socket: &UdpSocket, payload: &[u8], port: u16) -> usize {
    let mut sent = 0usize;
    for target in scan_candidates(&ipv4_interfaces()) {
        if socket.send_to(payload, (target, port)).is_ok() {
            sent += 1;
        }
    }
    sent
}

fn scan_candidates(interfaces: &[NetworkInterface]) -> Vec<Ipv4Addr> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for interface in interfaces {
        let mask = u32::from(interface.netmask);
        // /31 and /32 have no usable host range worth probing.
        if mask.count_ones() >= 31 {
            continue;
        }
        let network = u32::from(interface.ip) & mask;
        let broadcast = u32::from(interface.broadcast);
        let host_count = broadcast.saturating_sub(network).saturating_sub(1) as usize;
        let (range_start, range_end) = if host_count <= MAX_SCAN_HOSTS {
            (network.saturating_add(1), broadcast)
        } else {
            // A /16 or larger network would be impolite to sweep. The /24
            // around this address catches the usual home/office case; a fixed
            // peer can always be entered manually.
            let local_block = u32::from(interface.ip) & 0xffff_ff00;
            (
                local_block.saturating_add(1),
                local_block.saturating_add(255),
            )
        };
        for candidate in range_start..range_end {
            let ip = Ipv4Addr::from(candidate);
            if ip == interface.ip || ip.is_loopback() || ip.is_link_local() {
                continue;
            }
            if seen.insert(ip) {
                targets.push(ip);
                if targets.len() >= MAX_SCAN_HOSTS {
                    return targets;
                }
            }
        }
    }
    targets
}

pub fn default_port() -> u16 {
    DEFAULT_PORT
}

#[derive(Debug, Clone)]
pub struct LocalNetwork {
    pub ip: Option<String>,
    pub interface: Option<String>,
}

pub fn local_network() -> LocalNetwork {
    let mut candidates = ipv4_interfaces()
        .into_iter()
        .map(|interface| (interface.ip, interface.name))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(ip, name)| {
        let octets = ip.octets();
        let private = match octets {
            [10, ..] => true,
            [172, second, ..] if (16..=31).contains(&second) => true,
            [192, 168, ..] => true,
            _ => false,
        };
        let virtual_looking = name.to_lowercase().contains("vethernet")
            || name.to_lowercase().contains("virtual")
            || name.to_lowercase().contains("wsl");
        (!private, virtual_looking, name.clone())
    });
    match candidates.into_iter().next() {
        Some((ip, interface)) => LocalNetwork {
            ip: Some(ip.to_string()),
            interface: Some(interface),
        },
        None => LocalNetwork {
            ip: None,
            interface: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(ip: [u8; 4], netmask: [u8; 4]) -> NetworkInterface {
        let ip = Ipv4Addr::from(ip);
        let netmask = Ipv4Addr::from(netmask);
        NetworkInterface {
            name: "test".to_owned(),
            ip,
            netmask,
            broadcast: Ipv4Addr::from(u32::from(ip) | !u32::from(netmask)),
        }
    }

    #[test]
    fn local_network_does_not_return_loopback() {
        let network = local_network();
        if let Some(ip) = network.ip {
            assert!(!ip.starts_with("127."));
        }
    }

    #[test]
    fn a_slash_24_is_swept_without_the_local_host() {
        let iface = interface([192, 168, 10, 243], [255, 255, 255, 0]);
        let targets = scan_candidates(&[iface]);
        assert_eq!(targets.len(), 253);
        assert!(targets.contains(&Ipv4Addr::new(192, 168, 10, 1)));
        assert!(targets.contains(&Ipv4Addr::new(192, 168, 10, 254)));
        assert!(!targets.contains(&Ipv4Addr::new(192, 168, 10, 243)));
    }

    #[test]
    fn a_slash_16_is_capped_to_the_local_block() {
        let iface = interface([10, 4, 7, 22], [255, 255, 0, 0]);
        let targets = scan_candidates(&[iface]);
        assert!(targets.len() < MAX_SCAN_HOSTS);
        assert!(targets.contains(&Ipv4Addr::new(10, 4, 7, 1)));
        assert!(targets.contains(&Ipv4Addr::new(10, 4, 7, 254)));
        assert!(!targets.contains(&Ipv4Addr::new(10, 4, 7, 22)));
        assert!(!targets.contains(&Ipv4Addr::new(10, 4, 8, 1)));
    }

    #[test]
    fn a_link_local_interface_is_not_probed() {
        let iface = interface([169, 254, 3, 4], [255, 255, 0, 0]);
        assert!(scan_candidates(&[iface]).is_empty());
    }

    #[test]
    #[ignore = "requires a live LAN interface; run manually"]
    fn interface_bound_broadcast_is_received_locally() {
        let receiver = bind_discovery_socket(53995).expect("test port should bind");
        assert!(send_discovery(b"loco-test", 53995) > 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut received = false;
        while std::time::Instant::now() < deadline {
            let mut buffer = [0u8; 64];
            match receiver.recv_from(&mut buffer) {
                Ok((len, _)) if &buffer[..len] == b"loco-test" => {
                    received = true;
                    break;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
        assert!(received, "interface-bound broadcast did not loop back");
    }
}
