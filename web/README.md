# Visuara web UI

A React + Vite + Mantine single-page app that `visuara-signaling` embeds
into its binary at compile time (via `rust-embed`, see
`visuara-signaling/src/assets.rs`) and serves as the entire web UI: the
public site, login/registration, the device dashboard, and the admin panel.

## Building

`visuara-signaling` will not compile natively until `web/dist` exists,
since the embed path is resolved at `cargo build` time:

```sh
npm ci
npm run build
```

Run that once (or after frontend changes) before `cargo build -p
visuara-signaling` / `cargo run -p visuara-signaling`.

## Developing

For hot-reload against a live backend, run the signaling server and the
Vite dev server side by side:

```sh
# terminal 1, from the repo root
cargo run -p visuara-signaling

# terminal 2, from web/
npm run dev
```

The Vite dev server (default `http://localhost:5173`) proxies `/api/v1`
and `/ws` to the Rust server on port 8080 (see `vite.config.ts`), so cookies
and WebSocket connections work exactly as they would in production.
