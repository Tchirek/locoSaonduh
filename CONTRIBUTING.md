# Contributing

Keep every commit atomic: one idea, one working change, one commit.

Commit messages are original Japanese haiku, 5–7–5. Put the three lines in the
commit subject:

```text
春の夜や
窓辺にひかる
音の橋
```

Do not add conventional-commit prefixes or descriptions. The haiku is the
message.

Before sending a change:

```sh
npm ci
npm run check
npm run build
cd src-tauri
cargo fmt --check
cargo clippy --lib --tests -- -D warnings
cargo test --lib
```

The ignored network tests need real audio endpoints and UDP ports 53981 and
53982. Run them one at a time:

```sh
cd src-tauri
cargo test --test network_e2e -- --ignored --test-threads=1
```
