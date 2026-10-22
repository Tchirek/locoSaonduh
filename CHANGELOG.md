# Changelog

## 0.1.1 - 2026-10-23

- Fixed discovery when a VPN, virtual adapter, or multi-NIC setup owns the
  default route: multicast and directed broadcast now leave through every
  physical interface, not whichever interface wins the route table.
- Added a periodic unicast subnet sweep and symmetric probes, so two nodes can
  find each other even when all broadcast and multicast traffic is filtered.
- Added manual peer discovery by IP in Settings.
- Added Windows Firewall / AP-isolation guidance to both interface states and
  the documentation.
- Extended the peer expiry window to survive scan-only discovery networks.

## 0.1.0 - 2026-10-22

- First public release.
- Rust + Tauri 2 + Svelte 5.
- WASAPI loopback capture, Opus 10 ms frames, redundant datagrams, adaptive
  bitrate and jitter buffer, multi-source mixing.
- Windows, macOS Universal, and Linux release packages through GitHub Actions.
