<svelte:options runes={true} />
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api, onStateUpdate, type AppState } from "$lib/api";
  import { getVersion } from "@tauri-apps/api/app";
  import { check } from "@tauri-apps/plugin-updater";

  import LiveView from "$lib/views/LiveView.svelte";
  import ProfileView from "$lib/views/ProfileView.svelte";
  import LookupView from "$lib/views/LookupView.svelte";
  import SettingsView from "$lib/views/SettingsView.svelte";

  type Tab = "live" | "profile" | "lookup" | "settings";

  let activeTab = $state<Tab>("live");
  // Named `appState` (not `state`) — a `let state` collides with the `$state`
  // rune under Svelte 5 / svelte-check and gets parsed as store-subscription.
  let appState = $state<AppState>({
    connected: false,
    phase: "offline",
    map: "",
    map_image: "",
    players: [],
    self_name: "",
    error: "",
    updated: 0,
  });
  let alwaysOnTop = $state(false);
  let appVersion = $state("");
  let updateVersion = $state("");
  let updating = $state(false);

  async function installUpdate() {
    if (updating) return;
    updating = true;
    try {
      const u = await check();
      await u?.downloadAndInstall();
    } catch { updating = false; }
  }

  let unlisten: (() => void) | null = null;
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  onMount(async () => {
    // Subscribe to pushed state updates from the tracker loop
    unlisten = await onStateUpdate((s) => { appState = s; });

    // Fallback poll (catches the very first state before first push)
    try { appState = await api.getState(); } catch {}
    pollInterval = setInterval(async () => {
      try { appState = await api.getState(); } catch {}
    }, 3000);

    // Load always-on-top setting
    try {
      const s = await api.getSettings();
      alwaysOnTop = s.always_on_top;
    } catch {}

    try { appVersion = await getVersion(); } catch {}

    // Silently check for updates a few seconds after launch; surface a header
    // pill if one is available (never blocks — the user installs when they want).
    setTimeout(async () => {
      try {
        const u = await check();
        if (u?.available) updateVersion = u.version;
      } catch {}
    }, 4000);
  });

  onDestroy(() => {
    unlisten?.();
    if (pollInterval) clearInterval(pollInterval);
  });

  function switchTab(tab: Tab) {
    activeTab = tab;
  }

  // Jump to Lookup pre-filled with a Riot ID (from clicking a live player's name).
  // The nonce makes every click re-trigger, even for the same name.
  let lookupQuery = $state("");
  let lookupNonce = $state(0);
  function gotoLookup(riotId: string) {
    if (!riotId || !riotId.includes("#")) return;
    lookupQuery = riotId;
    lookupNonce++;
    activeTab = "lookup";
  }

  async function toggleAlwaysOnTop() {
    alwaysOnTop = !alwaysOnTop;
    try { await api.setAlwaysOnTop(alwaysOnTop); } catch {}
  }

  const dotClass = $derived(
    !appState.connected ? "dot off" :
    appState.phase === "ingame" || appState.phase === "pregame" ? "dot on" : "dot warn"
  );

  const statusText = $derived(
    !appState.connected ? "Waiting for VALORANT" :
    appState.phase === "ingame" ? "In game" :
    appState.phase === "pregame" ? "Agent select" : "In menus"
  );
</script>

<div class="app">
  <header>
    <div class="logo">
      <span class="mark">
        <svg viewBox="0 0 24 24">
          <path d="M4 6 L12 18 L20 6" fill="none" stroke="#ff2f43" stroke-width="3.4"
                stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </span>
      <span>Valo<b>Recon</b></span>
    </div>

    <nav>
      {#each (["live","profile","lookup","settings"] as Tab[]) as tab}
        <button
          class={activeTab === tab ? "active" : ""}
          onclick={() => switchTab(tab)}
        >
          {tab === "live" ? "Live Match" : tab === "profile" ? "Profile" :
           tab === "lookup" ? "Lookup" : "Settings"}
        </button>
      {/each}
    </nav>

    <div class="header-right">
      {#if updateVersion}
        <button class="update-pill" onclick={installUpdate} disabled={updating}
                title="Install update">
          {updating ? "Updating…" : `↑ Update to v${updateVersion}`}
        </button>
      {/if}
      <button
        class="pin-btn {alwaysOnTop ? 'pin-active' : ''}"
        title={alwaysOnTop ? "Unpin window" : "Pin on top"}
        onclick={toggleAlwaysOnTop}
        aria-label="Toggle always on top"
      >
        <svg viewBox="0 0 16 16" width="14" height="14">
          <path d="M9.5 1.5L14.5 6.5L11 8L8 11L6.5 9.5L3 13M10 6L6 10"
                fill="none" stroke="currentColor" stroke-width="1.6"
                stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <div class="status">
        <span class={dotClass}></span>
        <span>{statusText}</span>
      </div>
    </div>
  </header>

  {#if appVersion}
    <div class="version-badge">v{appVersion}</div>
  {/if}

  <main>
    <div class="view" class:active={activeTab === "live"}>
      <LiveView appState={appState} onLookup={gotoLookup} />
    </div>
    <div class="view" class:active={activeTab === "profile"}>
      {#if activeTab === "profile"}
        <ProfileView />
      {/if}
    </div>
    <div class="view" class:active={activeTab === "lookup"}>
      <LookupView initialQuery={lookupQuery} nonce={lookupNonce} />
    </div>
    <div class="view" class:active={activeTab === "settings"}>
      {#if activeTab === "settings"}
        <SettingsView />
      {/if}
    </div>
  </main>
</div>

<style>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
}

header {
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 12px 20px;
  position: relative;
  border-bottom: 1px solid var(--line);
  background: linear-gradient(180deg, #15171f, #101218);
  flex-shrink: 0;
}
header::after {
  content: "";
  position: absolute;
  left: 0; right: 0; bottom: -1px;
  height: 2px;
  background: linear-gradient(90deg, var(--accent), transparent 42%);
}

.logo {
  display: flex;
  align-items: center;
  gap: 10px;
  font-weight: 900;
  letter-spacing: .5px;
  font-size: 15px;
  text-transform: uppercase;
  white-space: nowrap;
}
.logo .mark {
  width: 24px; height: 24px;
  border-radius: 7px;
  background: var(--accent);
  display: grid;
  place-items: center;
  box-shadow: 0 0 16px -2px var(--accent);
  flex-shrink: 0;
}
.logo .mark svg { width: 14px; height: 14px; }
.logo b { color: var(--accent); }

nav {
  display: flex;
  gap: 2px;
  margin-left: 6px;
}
nav button {
  background: none;
  border: 0;
  color: var(--muted);
  padding: 8px 14px;
  border-radius: 8px;
  cursor: pointer;
  font: inherit;
  font-weight: 800;
  letter-spacing: .3px;
  transition: color .15s, background .15s;
  text-transform: uppercase;
  font-size: 12.5px;
}
nav button:hover { color: var(--text); background: var(--panel2); }
nav button.active {
  color: #fff;
  background: color-mix(in srgb, var(--accent) 16%, var(--panel2));
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent);
}

.header-right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 9px;
}

.update-pill {
  background: color-mix(in srgb, var(--accent) 20%, var(--panel2));
  border: 1px solid color-mix(in srgb, var(--accent) 55%, transparent);
  color: #fff;
  font-weight: 800;
  font-size: 11.5px;
  text-transform: uppercase;
  letter-spacing: .3px;
  padding: 6px 12px;
  border-radius: 8px;
  cursor: pointer;
  white-space: nowrap;
  box-shadow: 0 0 14px -4px var(--accent);
  transition: background .15s;
  animation: pillin .3s ease;
}
.update-pill:hover { background: color-mix(in srgb, var(--accent) 32%, var(--panel2)); }
.update-pill:disabled { opacity: .6; cursor: default; }
@keyframes pillin { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }

.pin-btn {
  background: none;
  border: 1px solid var(--line2);
  color: var(--dim);
  padding: 6px 8px;
  border-radius: 7px;
  display: grid;
  place-items: center;
  transition: color .15s, border-color .15s, background .15s;
}
.pin-btn:hover { color: var(--text); border-color: var(--line2); }
.pin-btn.pin-active {
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
}

.status {
  display: flex;
  align-items: center;
  gap: 9px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
  padding: 6px 13px;
  border-radius: 8px;
  text-transform: uppercase;
  letter-spacing: .4px;
  background: var(--panel2);
  box-shadow: inset 0 0 0 1px var(--line);
  white-space: nowrap;
}
.dot {
  width: 8px; height: 8px;
  border-radius: 50%;
  background: #4b5160;
  flex-shrink: 0;
}
.dot.on {
  background: var(--good);
  box-shadow: 0 0 10px 1px color-mix(in srgb, var(--good) 70%, transparent);
}
.dot.warn {
  background: var(--warn);
  box-shadow: 0 0 10px 1px color-mix(in srgb, var(--warn) 60%, transparent);
}
.dot.off { background: #4b5160; }

.version-badge {
  position: fixed;
  bottom: 7px;
  right: 10px;
  font-size: 10px;
  font-weight: 700;
  color: var(--dim);
  letter-spacing: .4px;
  pointer-events: none;
  z-index: 999;
}

main {
  flex: 1;
  overflow: hidden;
  position: relative;
}

.view {
  display: none;
  position: absolute;
  inset: 0;
  overflow-y: auto;
  padding: 20px 22px 26px;
  animation: fade .2s ease;
}
.view.active { display: block; }

@keyframes fade {
  from { opacity: 0; transform: translateY(4px); }
  to   { opacity: 1; transform: none; }
}
</style>
