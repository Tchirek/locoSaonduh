# Third-party software

locoSaonduh is MIT-licensed. Its main runtime dependencies carry their own
licenses:

| Component | License | Use |
| --- | --- | --- |
| [Tauri](https://tauri.app/) | Apache-2.0 / MIT | desktop shell and WebView bridge |
| [Svelte](https://svelte.dev/) | MIT | UI runtime |
| [Lucide](https://lucide.dev/) | ISC | interface icons |
| [cpal](https://github.com/RustAudio/cpal) | Apache-2.0 | audio device I/O and WASAPI loopback |
| [opus-pure](https://github.com/stephenberry/opus-pure) | BSD-3-Clause | Opus encoder and decoder |
| [rtrb](https://github.com/mgeier/rtrb) | MIT / Apache-2.0 | realtime-safe sample ring buffers |
| [serde](https://serde.rs/) | MIT / Apache-2.0 | wire and settings serialization |

The full dependency graph and exact versions are pinned in `src-tauri/Cargo.lock`
and `package-lock.json`.
