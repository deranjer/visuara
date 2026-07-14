# Client templates

Drop pre-built release client binaries here for the server's `/download` endpoint to serve (with the configured server URL/device name patched in). There's no CI or cross-compilation pipeline yet, so these are built and placed here manually.

Expected filenames:

- `windows-x86_64.exe` — built on a real Windows machine: `cargo build --release -p visuara-client`, then copy `target/release/visuara-client.exe` here under this name.
- `linux-x86_64` — built on a real Linux machine: `cargo build --release -p visuara-client`, then copy `target/release/visuara-client` here under this name.

A platform is only listed as available on the public `/download` page once its file exists here. This directory is mounted read-only into the `signaling` container (see `docker-compose.yml`).
