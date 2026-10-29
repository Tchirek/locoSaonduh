package com.tchirek.locosaonduh.engine

import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.MulticastSocket
import java.net.NetworkInterface
import java.net.SocketException
import java.util.Collections

class NetworkInterfaceInfo(
    val name: String,
    val networkInterface: NetworkInterface,
    val ip: InetAddress,
    val netmask: Int,
    val broadcast: InetAddress,
)

// 垣根越し 灯の届くを 数へけり
fun ipv4Interfaces(): List<NetworkInterfaceInfo> {
    val result = mutableListOf<NetworkInterfaceInfo>()
    val all = try {
        Collections.list(NetworkInterface.getNetworkInterfaces())
    } catch (_: Exception) {
        emptyList()
    }
    for (network in all) {
        if (!network.isUp || network.isLoopback) continue
        val addresses = try {
            network.interfaceAddresses
        } catch (_: Exception) {
            continue
        }
        for (address in addresses) {
            val ip = address.address
            if (ip !is Inet4Address) continue
            if (ip.isLoopbackAddress || ip.isLinkLocalAddress) continue
            val mask = address.networkPrefixLength.toInt()
            if (mask >= 31) continue
            val ipValue = toInt(ip)
            val netmask = if (mask == 0) 0 else (-1 shl (32 - mask))
            val derived = intToAddress(ipValue or netmask.inv())
            val broadcast = address.broadcast?.takeIf { it is Inet4Address } ?: derived
            result.add(NetworkInterfaceInfo(network.name, network, ip, netmask, broadcast))
        }
    }
    result.sortBy { toInt(it.ip) }
    val seen = mutableSetOf<String>()
    return result.filter { seen.add(it.ip.hostAddress ?: "") }
}

class LocalNetwork(val ip: String?, val interfaceName: String?)

// うららかに 己の住むを 定めけり
fun localNetwork(): LocalNetwork {
    val candidates = ipv4Interfaces().map { it.ip to it.name }
    if (candidates.isEmpty()) return LocalNetwork(null, null)
    val sorted = candidates.sortedWith(
        compareBy(
            { (ip, _) -> !isPrivate(ip) },
            { (_, name) -> looksVirtual(name) },
            { (_, name) -> name },
        ),
    )
    val (ip, name) = sorted.first()
    return LocalNetwork(ip.hostAddress, name)
}

private fun isPrivate(ip: InetAddress): Boolean {
    val value = toInt(ip)
    val first = (value ushr 24) and 0xFF
    val second = (value ushr 16) and 0xFF
    return first == 10 ||
        (first == 172 && second in 16..31) ||
        (first == 192 && second == 168)
}

private fun looksVirtual(name: String): Boolean {
    val lower = name.lowercase()
    return lower.contains("veth") ||
        lower.contains("virtual") ||
        lower.contains("wsl") ||
        lower.contains("rmnet") ||
        lower.contains("tun") ||
        lower.contains("dummy")
}

fun toInt(address: InetAddress): Int {
    var value = 0
    for (byte in address.address) {
        value = (value shl 8) or (byte.toInt() and 0xFF)
    }
    return value
}

fun intToAddress(value: Int): InetAddress {
    val bytes = byteArrayOf(
        (value ushr 24).toByte(),
        (value ushr 16).toByte(),
        (value ushr 8).toByte(),
        value.toByte(),
    )
    return InetAddress.getByAddress(bytes)
}

class DiscoverySocket(val socket: MulticastSocket, val port: Int)

class DiscoveryException(message: String, cause: Throwable? = null) : Exception(message, cause)

// 古株の 去るを待ちつつ 次の戸へ
fun bindDiscovery(port: Int, patienceMs: Long): DiscoverySocket {
    val deadline = System.currentTimeMillis() + patienceMs
    while (true) {
        try {
            return DiscoverySocket(bindOne(port), port)
        } catch (error: Exception) {
            if (System.currentTimeMillis() >= deadline) break
            Thread.sleep(100)
        }
    }
    for (candidate in (port + 1)..(port + 9)) {
        try {
            val socket = bindOne(candidate)
            return DiscoverySocket(socket, socket.localPort)
        } catch (_: Exception) {
        }
    }
    val socket = bindOne(0)
    return DiscoverySocket(socket, socket.localPort)
}

private fun bindOne(port: Int): MulticastSocket {
    val socket = MulticastSocket(null)
    try {
        socket.reuseAddress = false
        socket.broadcast = true
        socket.timeToLive = 1
        socket.loopbackMode = false
        try {
            socket.sendBufferSize = 1 shl 20
            socket.receiveBufferSize = 1 shl 20
        } catch (_: SocketException) {
        }
        socket.trafficClass = 46 shl 2
        socket.soTimeout = 100
        socket.bind(InetSocketAddress(port))
        var joined = 0
        for (info in ipv4Interfaces()) {
            try {
                socket.networkInterface = info.networkInterface
                socket.joinGroup(InetSocketAddress(Wire.MULTICAST, port), info.networkInterface)
                joined += 1
            } catch (_: Exception) {
            }
        }
        if (joined == 0) {
            try {
                socket.joinGroup(InetSocketAddress(Wire.MULTICAST, port), null)
            } catch (_: Exception) {
            }
        }
        try {
            socket.networkInterface = null
        } catch (_: Exception) {
        }
        return socket
    } catch (error: Exception) {
        try {
            socket.close()
        } catch (_: Exception) {
        }
        throw DiscoveryException("UDP $port 无法绑定", error)
    }
}

// 幾筋の 道へ同じ灯 配りけり
fun sendDiscovery(payload: ByteArray, port: Int): Int {
    val interfaces = ipv4Interfaces()
    var sent = 0
    if (interfaces.isEmpty()) {
        try {
            DatagramSocket().use { socket ->
                socket.broadcast = true
                val targets = listOf(Wire.MULTICAST, InetAddress.getByName("255.255.255.255"))
                for (target in targets) {
                    try {
                        socket.send(DatagramPacket(payload, payload.size, target, port))
                        sent += 1
                    } catch (_: Exception) {
                    }
                }
            }
        } catch (_: Exception) {
        }
        return sent
    }
    for (info in interfaces) {
        try {
            DatagramSocket(InetSocketAddress(info.ip, 0)).use { socket ->
                socket.broadcast = true
                socket.trafficClass = 46 shl 2
                for (target in listOf(Wire.MULTICAST, InetAddress.getByName("255.255.255.255"), info.broadcast)) {
                    if (target == info.ip) continue
                    try {
                        socket.send(DatagramPacket(payload, payload.size, target, port))
                        sent += 1
                    } catch (_: Exception) {
                    }
                }
            }
        } catch (_: Exception) {
        }
    }
    return sent
}

const val MAX_SCAN_HOSTS = 1_024

// 露の世を ひと巡りして 名を呼ばむ
fun scanCandidates(): List<InetAddress> {
    val targets = mutableListOf<InetAddress>()
    val seen = HashSet<String>()
    for (info in ipv4Interfaces()) {
        val mask = info.netmask
        if (Integer.bitCount(mask) >= 31) continue
        val network = toInt(info.ip) and mask
        val broadcast = toInt(info.broadcast)
        val hostCount = (broadcast - network - 1).coerceAtLeast(0)
        val range: IntRange
        if (hostCount <= MAX_SCAN_HOSTS) {
            range = (network + 1)..broadcast
        } else {
            val localBlock = toInt(info.ip) and 0xFFFF_FF00.toInt()
            range = (localBlock + 1)..(localBlock + 255)
        }
        for (candidate in range) {
            val ip = intToAddress(candidate)
            if (ip == info.ip || ip.isLoopbackAddress || ip.isLinkLocalAddress) continue
            if (seen.add(ip.hostAddress ?: continue)) {
                targets.add(ip)
                if (targets.size >= MAX_SCAN_HOSTS) return targets
            }
        }
    }
    return targets
}

fun scanDiscovery(socket: DatagramSocket, payload: ByteArray, port: Int): Int {
    var sent = 0
    for (target in scanCandidates()) {
        try {
            socket.send(DatagramPacket(payload, payload.size, target, port))
            sent += 1
        } catch (_: Exception) {
        }
    }
    return sent
}
