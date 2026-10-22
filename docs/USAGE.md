# 使用与构建

## 环境

已验证的日常目标是 Windows 10/11。构建需要：

- Node.js 22+
- Rust 1.88+
- Visual Studio Build Tools（MSVC，含 Windows SDK）
- WebView2 Runtime（Windows 11 自带；Windows 10 可能需要安装）

macOS / Linux 使用同一套 Tauri 与 cpal 代码，但没有在真实音频硬件上做端到端验证。Linux 上“系统声音”通常要选一个 monitor / loopback 输入，而不是普通扬声器。

## 本地运行

```sh
npm ci
npm run tauri dev
```

首次启动时 Windows 防火墙会询问网络访问。允许专用网络即可。两台设备必须处在同一个可互通的 Wi-Fi / 有线局域网内。

## 发布构建

本地构建：

```sh
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/`。音频热路径不依赖调试构建，正式使用建议用 release。

## GitHub 自动打包

推送任意 `v*` 标签，或在 Actions 里手动运行 `release` 工作流，会并行生成：

- Windows x64：NSIS 安装包
- macOS Universal：Intel 与 Apple Silicon 通用的 DMG
- Linux x64：AppImage 与 `.deb`

构建完成后自动发布到 GitHub Releases。包未签名，Windows SmartScreen 与 macOS Gatekeeper 可能要求确认；这不影响功能。macOS 还要求系统录制/本地网络权限。

```sh
git tag v0.1.0
git push origin v0.1.0
```

## 发送

1. 在发送设备打开「发送」。
2. 选择「系统声音」或一个麦克风。系统声音会采集该输出端点上的所有声音。
3. 点「开始广播」。
4. 让接收设备打开「接收」，在那台设备的「附近广播」里点「收听」。

一台发送端可以同时被多台设备收听，最多 24 台上报连接。听众列表会显示每一台的缓冲、抖动与丢包。

广播期间这台设备不能再收听其他设备，避免本机声音回环。

## 接收

1. 打开「接收」。附近正在广播的发送端会自动出现。
2. 如果想换一个耳机或音响，先在设置里选择「接收输出」。默认跟随系统输出。
3. 点一路广播的「收听」。可以继续加入其他广播。
4. 多路声音会混合后从同一输出播放。

正在播放区域会展示每一路真实的缓冲、码率、抖动和丢包。「断开」只移除一路，其他声音继续播放。

## 延迟与音质

默认是自动策略，不需要调参数。大致构成：

```text
采集 / 设备缓冲  ~10–30 ms
Opus 10 ms 帧     ~10–13 ms
网络传输          ~1–20 ms（局域网）
自适应抖动缓冲     30–120 ms
输出设备与混音     ~20–40 ms
```

实际值取决于系统音频驱动、Wi-Fi 和设备采样率。界面里的数字来自当前会话，不是预估值。

链路干净且持续稳定时，发送端会升到 160 kbps 高保真档；出现丢包或抖动时先降到 64–112 kbps，避免为了音质换来回声或断续。接收端宁愿短暂多缓冲一些，也不让声音连续爆音。

## 发现不到设备

两端都启动后，最多等 5 秒，或用页面上的「扫描」。

- 确认两台设备在同一网段，而不是一个在 VPN / 访客网络、另一个在主网络。
- 关闭路由器的 AP isolation / 客户端隔离。
- 允许应用通过 Windows 防火墙的专用网络；需要放行 UDP 入站端口 `53410`。两台设备都要允许。被拒绝过一次后，Windows 不会再次弹窗，需要在“防火墙和网络保护 → 允许应用通过防火墙”里重新勾选专用网络。
  - 管理员 PowerShell 可直接添加规则：
    `New-NetFirewallRule -DisplayName "locoSaonduh UDP" -Direction Inbound -Protocol UDP -LocalPort 53410 -Profile Private -Action Allow`
- 虚拟网卡、Clash / FlClash、Steam Streaming 等会暴露多个接口。v0.1.1 会绑定每个物理接口发送发现包并扫描子网，正常不会再被 VPN 的默认路由吞掉；界面底部显示当前选中的局域网地址，对端应能访问这个地址。
- 端口 `53410` 被其他程序占用时，应用会启动失败并显示原因。

## 找不到系统声音

音源列表中的“系统”是输出端点。Windows 上应用通过 WASAPI 回环采集它。

- 确认应用发送的设备和系统正在播放的设备一致。
- VB-Audio、Steam Streaming、虚拟显示器之类的端点可能是无声的；换成真实耳机 / 音响端点再试。
- 蓝牙耳机的系统输出常常有额外缓冲；需要更低延迟时优先用有线或 5 GHz Wi-Fi。

## 测试

常规检查：

```sh
npm run check
npm run build
cd src-tauri
cargo fmt --all -- --check
cargo clippy --lib --tests -- -D warnings
cargo test --lib
```

真实音频 / UDP 测试需要本机有音频端点，并使用默认局域网端口之外的两个测试端口：

```sh
cd src-tauri
cargo test --test network_e2e -- --ignored --test-threads=1
```

这两条测试会打开真实采集与播放设备。其中一条发送 2.4 秒的极低音量测试音。

## 隐私

音频不经过云端，不上传文件，不生成账号。协议不加密，任何能访问 UDP 53410 的局域网设备都可以在广播期间订阅；不要在公共 Wi-Fi 上广播敏感内容。
