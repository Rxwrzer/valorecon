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

## Download

Grab the latest installer from [Releases](https://github.com/Rxwrzer/valorecon/releases):

- **`ValoRecon_x64-setup.exe`** — installs per-user, no admin required. The app auto-updates from here on.

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

## Privacy & Safety

**Not affiliated with or endorsed by Riot Games.** ValoRecon is an independent, open-source project.

**Is it safe / can it get me banned?** ValoRecon is designed to stay well clear of anything an anti-cheat cares about:

- **Read-only.** It never injects into the VALORANT process, reads no game memory, and modifies no game files. It runs as its own normal desktop window — there is **no overlay injected into the game**, which is a deliberate choice to avoid any interaction with Vanguard.
- **Same data the client already exposes.** Live lobby data comes from VALORANT's **local client API** — the same local endpoint the game's own client UI reads from. ValoRecon reads the lockfile VALORANT writes to disk while running; it does not touch your Riot password or auth beyond that local handshake.
- **No automation.** It does not play, click, aim, or act in-game in any way. It only displays information.

Use of any third-party tool is at your own discretion — but ValoRecon takes the most conservative, hands-off approach possible.

**What data leaves your machine:**

- **Nothing, by default.** With no HenrikDev key set, everything stays local — live lobby, ranks, and stats all come from your own Riot local API.
- **Only when you use Lookup or opt into deeper stats:** the Riot ID you search (and your HenrikDev key) are sent to the HenrikDev API to fetch that player's public match history. Agent/map/rank images are fetched from valorant-api.com.

**What's stored locally:**

- HenrikDev API key and settings → `%APPDATA%\ValoRecon\config.json`
- Cached match history and stats → `%APPDATA%\ValoRecon\`
- No account creation, no cloud sync, no telemetry, no analytics.

## License

MIT — see [LICENSE](LICENSE)
