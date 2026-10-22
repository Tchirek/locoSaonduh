use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;

use crate::protocol::{DEFAULT_PORT, MULTICAST_ADDR};

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
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())
        .with_context(|| format!("UDP port {port} is already in use"))?;
    // The session loop polls this socket and must also publish state, answer
    // commands and send feedback while packets are flowing. A blocking read
    // would let a continuous stream starve all of that.
    socket
        .set_nonblocking(true)
        .context("could not make discovery socket non-blocking")?;
    let socket: UdpSocket = socket.into();
    // A missing multicast route must not prevent unicast discovery from
    // working, so this is best-effort.
    if let Err(error) = socket.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED) {
        log::warn!("multicast join failed; broadcast fallback remains active: {error}");
    }
    Ok(Arc::new(socket))
}

pub fn send_discovery(socket: &UdpSocket, payload: &[u8], port: u16) {
    let multicast = SocketAddr::from((MULTICAST_ADDR, port));
    let broadcast = SocketAddr::from((Ipv4Addr::BROADCAST, port));
    if let Err(error) = socket.send_to(payload, multicast) {
        log::debug!("multicast send failed: {error}");
    }
    if let Err(error) = socket.send_to(payload, broadcast) {
        log::debug!("broadcast send failed: {error}");
    }
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
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return LocalNetwork {
            ip: None,
            interface: None,
        };
    };
    let mut candidates = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(address) if !address.ip.is_link_local() => {
                Some((address.ip.to_string(), interface.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(ip, name)| {
        let private = ip.starts_with("192.168.")
            || ip.starts_with("10.")
            || ip.starts_with("172.16.")
            || ip.starts_with("172.17.")
            || ip.starts_with("172.18.")
            || ip.starts_with("172.19.")
            || ip.starts_with("172.2")
            || ip.starts_with("172.30.")
            || ip.starts_with("172.31.");
        let virtual_looking = name.to_lowercase().contains("vethernet")
            || name.to_lowercase().contains("virtual")
            || name.to_lowercase().contains("wsl");
        (!private, virtual_looking, name.clone())
    });
    match candidates.into_iter().next() {
        Some((ip, interface)) => LocalNetwork {
            ip: Some(ip),
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

    #[test]
    fn local_network_does_not_return_loopback() {
        let network = local_network();
        if let Some(ip) = network.ip {
            assert!(!ip.starts_with("127."));
        }
    }
}
