# Visuara

Visuara is a self-hosted remote-desktop tool — a TeamViewer-style replacement you run on your own server. It's built for ordinary desktop work (screen sharing, remote control, clipboard sync, file transfer), not game-streaming performance.

- **Server:** a small signaling/relay service you run in Docker on your own infrastructure. It never sees your screen or input — it only brokers WebRTC connections between clients, which are end-to-end encrypted (DTLS) whether they connect directly (peer-to-peer) or fall back through the TURN relay.
- **Client:** one native app (Windows/Linux) that acts as either the **host** (the machine being viewed/controlled) or the **controller** (the machine doing the viewing/controlling), picked in the GUI or via CLI flags.

## How a session works

1. Whoever wants to be controlled clicks **Start sharing** on the Host panel. This registers (or logs into) an account on your server and gets back a **device ID** and a one-time password (OTP).
2. Whoever wants to control that machine enters the device ID + OTP on the Connect panel.
3. The two clients negotiate a direct WebRTC connection (via your signaling server), falling back to the TURN relay if a direct P2P path isn't possible (e.g. both sides behind restrictive NATs).
4. Once connected: video streams host→controller, input events flow controller→host, clipboard text syncs both ways, and files can be dragged onto the controller's window to send them to the host.

## Running the server

Requires Docker and an existing [Caddy](https://caddyserver.com/) instance using [docker-labels](https://github.com/lucaslorentz/caddy-docker-proxy) for reverse-proxying (the signaling service is fronted by Caddy; the bundled `coturn` TURN relay is not, since raw UDP relay allocations don't work through an HTTP/TCP proxy).

```bash
cd docker
cp .env.example .env
# edit .env — see comments in that file for what each variable means
docker compose --env-file .env up -d --build
```

This starts two containers:
- `signaling` — the Visuara server itself (accounts, device registry, pairing, WebRTC signaling, and the web UI — a React app the server embeds and serves directly), reverse-proxied by Caddy at `wss://${SIGNALING_HOSTNAME}`.
- `coturn` — a TURN relay for clients that can't connect directly, using host networking and a shared HMAC secret with the signaling server (no separate coturn account setup needed).

Data (the SQLite database) persists in the `signaling-data` Docker volume across restarts/upgrades.

### Environment variables (`docker/.env`)

| Variable | Purpose |
|---|---|
| `TURN_SHARED_SECRET` | Shared between `signaling` and `coturn` so the server can mint short-lived TURN credentials. Generate with `openssl rand -hex 32`. |
| `PUBLIC_HOST` | Your server's public hostname/IP — used as the coturn realm and in the TURN URL handed to clients. |
| `PUBLIC_IP` | The real internet-facing IP `coturn` advertises in relay candidates (not a private/LAN address). |
| `SIGNALING_HOSTNAME` | Hostname Caddy routes to the signaling service, e.g. `visuara.example.com`. |

Also open/forward UDP port `3478` (STUN/TURN) and the UDP range `49152–65535` (TURN relay allocations) to the server, in addition to whatever port Caddy already uses for HTTPS.

### Admin access

There's no separate admin password. **The first account ever registered through the web UI automatically becomes the admin account**, and public self-service registration closes immediately afterward. From then on, the admin can create additional accounts and/or re-enable public registration from the **Admin** section of the web UI.

> **Upgrading an existing deployment?** If your database already has accounts from before this per-account admin model existed, none of them is flagged as admin. Bootstrap one manually against the running container's database, then log in through the web UI:
> ```bash
> docker compose exec signaling sqlite3 /data/visuara.db \
>   "UPDATE accounts SET role = 'admin' WHERE email = 'you@example.com';"
> ```

### Option A: pre-configured downloads (recommended)

Once the server is running, register the first account (it becomes admin) and set a default server URL (and optionally a default device name) under **Admin → Settings**. Anyone can then get a ready-to-run client — no typing in a server URL — from the public download page:

```
https://<your-signaling-hostname>/download
```

This works by patching the server URL directly into a pre-built binary (no compilation happens on the server), so you first need to place built binaries where the server can find them — see `docker/client-templates/README.md`. The easiest source for these is a tagged release (see below): download `windows-x86_64.exe` / `linux-x86_64` from the release's GitHub Release page and drop them into `docker/client-templates/` on the machine running the `signaling` container.

### Option B: build/download the client yourself

Grab `windows-x86_64.exe` or `linux-x86_64` from the [Releases](https://github.com/deranjer/visuara/releases) page, or build it yourself:

```bash
cargo build --release -p visuara-client
```

Linux build/runtime notes: capture and input injection target **X11 only** in v1 (no Wayland support yet).

## Using the client

Running the binary with no arguments launches the GUI, which has two panels:

- **Host** — enter the account email/password to use for this machine (auto-registers on first use, logs in afterward) and a device name, then **Start sharing**. This shows the device ID and OTP a controller needs, plus controls for [unattended access](#unattended-access).
- **Connect** — enter the same account's email/password, the target device ID, and its current OTP (or check "use fixed unattended password" and enter that instead) to view/control that machine.

The same binary also has CLI subcommands for scripting/headless use:

```bash
# Host this machine (registers/logs in, prints device ID + OTP, waits for connections)
visuara-client host --email you@example.com --password ... --device-name my-desktop

# Connect to a host by device ID + OTP
visuara-client controller --email you@example.com --password ... --target-device-id <id> --otp <otp>

# ...or by its fixed unattended password instead of a fresh OTP
visuara-client controller --email you@example.com --password ... --target-device-id <id> --otp <fixed-password> --unattended
```

`--server-url` is optional on both if the binary already has one embedded (see Option A above); otherwise it defaults to `ws://127.0.0.1:8080/ws`.

### Unattended access

From the Host panel, once a device is registered, you can:
- **Enable auto-start on login** — saves the account credentials locally and registers this app to relaunch as a host automatically after you log in (Windows: a per-user `Run` registry entry; Linux: a `systemd --user` unit). This is intentionally lighter than a full OS service: v1 doesn't support controlling the login/lock screen, so a real system-level service that starts before login wouldn't gain anything. A user still has to be logged into a desktop session for the host to be reachable.
- **Set a fixed password** — stored on the server for this device, lets a controller connect without needing a fresh OTP each time (GUI: check "use fixed unattended password" on the Connect panel; CLI: pass `--unattended`).

## Feature scope (v1)

- Screen viewing/control, one monitor shared at a time (switchable mid-session from the controller).
- Clipboard sync: **text only**.
- File transfer: drag a file onto the controller's window to send it to the host (one direction, no remote file browser yet).
- Video: H.264 over a WebRTC data track — tuned for ordinary desktop work, not low-latency game streaming.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for the branching model (GitFlow) and how releases are cut. Quick start for local development:

```bash
# visuara-signaling embeds the built web UI into its binary, so build that first
npm --prefix web ci
npm --prefix web run build

cargo build --workspace
cargo test --workspace
```

See [web/README.md](web/README.md) for the web UI's own dev loop (Vite dev server with hot-reload against a live `cargo run` backend).

The workspace has four crates: `visuara-common` (shared wire protocol), `visuara-signaling` (the server, embedding and serving the `web/` React app), `visuara-agent` (capture/input/encode/decode), and `visuara-client` (the GUI + CLI, links the other two).
