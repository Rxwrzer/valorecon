# ValoRecon

A lightweight native Windows app for tracking VALORANT lobbies in real time. See ranks, recent stats, and party groupings for every player in agent select and in-game — built with Tauri (Rust + Svelte).

> Not affiliated with or endorsed by Riot Games.

---

## Features

- **Live match** — ranks, RR, peak rank, KDA / ACS / ADR / HS% / WR per player, party detection
- **Profile** — your RR history sparkline, act and lifetime stats, deep game history pull
- **Player lookup** — search any player by Riot ID via HenrikDev API
- **Smart rate limiting** — shared budget across all API calls; Riot and HenrikDev run in parallel
- **Always-on-top** — pin above VALORANT for quick reference

## Screenshots

| Live Match | Profile |
|---|---|
| ![Live match view showing ranks and stats for all 10 players](docs/live.png) | ![Profile view showing RR history sparkline and act stats](docs/profile.png) |

## Download

Grab the latest installer from [Releases](https://github.com/Rxwrzer/valorecon/releases):

- **`ValoRecon_x64-setup.exe`** — recommended (installs per-user, no admin required)
- **`ValoRecon_x64_en-US.msi`** — MSI for managed environments

## Setup

1. Install ValoRecon
2. Launch VALORANT and log in
3. Open ValoRecon — it connects automatically via the Riot local API

**For player lookup and deeper live stats (optional):**

1. Get a free API key from the [HenrikDev Discord](https://discord.gg/henrikdev)
2. Paste it in Settings → HenrikDev API key
3. Optionally enable *Use HenrikDev for deeper live stats* for broader historical averages

## How it works

ValoRecon reads from VALORANT's local client API (the same endpoint the game itself uses for the client UI). No credentials are stored beyond your machine — the app reads the lockfile VALORANT writes to disk while running.

**Data sources:**
| Data | Source |
|---|---|
| Live lobby, ranks, match history | Riot local client API |
| Player lookup, stored match history | HenrikDev API (optional, requires free key) |
| Agent/map/rank assets | [valorant-api.com](https://valorant-api.com) |

**Rate limits:** Riot's local API allows ~45 requests/60s. ValoRecon tracks this with a shared sliding-window limiter and never sends more. Stats for all 10 lobby players fill in parallel within ~2 minutes on a fresh lobby; subsequent games reuse cached data.

## Building from source

Requirements: [Rust](https://rustup.rs) · [Node.js 18+](https://nodejs.org) · [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
git clone https://github.com/Rxwrzer/valorecon
cd valorecon
npm install
npm run tauri dev       # development (hot-reload)
npm run tauri build     # release build → src-tauri/target/release/bundle/
```

## Privacy

- No account creation, no cloud sync, no telemetry
- Your HenrikDev API key is stored locally in `%APPDATA%\ValoRecon\config.json`
- Match data is cached locally in `%APPDATA%\ValoRecon\` and never leaves your machine

## License

MIT — see [LICENSE](LICENSE)
