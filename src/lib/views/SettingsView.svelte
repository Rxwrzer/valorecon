<svelte:options runes={true} />
<script lang="ts">
  import { api, type AppSettings } from "$lib/api";
  import { check } from "@tauri-apps/plugin-updater";

  let settings = $state<AppSettings | null>(null);
  let keyInput = $state("");
  let regionValue = $state("");
  let pollSeconds = $state(7);
  let henrikRateLimit = $state(30);
  let liveUseHenrik = $state(false);
  let saved = $state(false);
  let saving = $state(false);
  let updateStatus = $state<"idle" | "checking" | "available" | "uptodate" | "installing" | "error">("idle");
  let updateVersion = $state("");

  async function load() {
    try {
      settings = await api.getSettings();
      regionValue = settings.region_override || "";
      pollSeconds = settings.poll_seconds || 7;
      henrikRateLimit = settings.henrik_rate_limit ?? 30;
      liveUseHenrik = settings.live_use_henrik ?? false;
    } catch {}
  }
  load();

  async function save() {
    saving = true;
    try {
      await api.saveSettings({
        henrik_key: keyInput,
        region_override: regionValue,
        poll_seconds: pollSeconds,
        henrik_rate_limit: henrikRateLimit,
        live_use_henrik: liveUseHenrik,
      });
      keyInput = "";
      saved = true;
      setTimeout(() => { saved = false; }, 2000);
      await load();
    } catch {}
    saving = false;
  }

  async function checkUpdate() {
    updateStatus = "checking";
    try {
      const update = await check();
      if (update?.available) {
        updateVersion = update.version;
        updateStatus = "available";
      } else {
        updateStatus = "uptodate";
      }
    } catch {
      updateStatus = "error";
    }
  }

  async function installUpdate() {
    updateStatus = "installing";
    try {
      const update = await check();
      await update?.downloadAndInstall();
    } catch {
      updateStatus = "error";
    }
  }
</script>

<div class="mhead"><h2>Settings</h2></div>

{#if settings}
  <div class="form">
    <div>
      <label for="henrik-key">HenrikDev API key <span class="muted">(only needed for player Lookup)</span></label>
      <input
        id="henrik-key"
        bind:value={keyInput}
        placeholder={settings.has_key
          ? `key set (${settings.key_hint}) — leave blank to keep`
          : "paste key to enable lookup"}
      />
      <div class="muted" style="font-size:12px;margin-top:6px">
        Free key from the HenrikDev Discord. Leave blank to keep the current one.
      </div>
    </div>

    <div>
      <label for="region-select">Region</label>
      <select id="region-select" bind:value={regionValue}>
        <option value="">Auto-detect</option>
        <option value="na">NA</option>
        <option value="eu">EU</option>
        <option value="ap">AP</option>
        <option value="kr">KR</option>
        <option value="latam">LATAM</option>
        <option value="br">BR</option>
      </select>
    </div>

    <div>
      <label for="poll-seconds">Refresh interval (seconds)</label>
      <input id="poll-seconds" type="number" min="3" max="60" bind:value={pollSeconds} />
    </div>

    <div>
      <label for="henrik-rate-limit">HenrikDev requests per minute <span class="muted">(10–300, default 30)</span></label>
      <input id="henrik-rate-limit" type="number" min="10" max="300" bind:value={henrikRateLimit} />
    </div>

    <div class="toggle-row">
      <input id="live-use-henrik" type="checkbox" bind:checked={liveUseHenrik} />
      <label for="live-use-henrik">Use HenrikDev for deeper live stats <span class="muted">(requires API key)</span></label>
    </div>

    <div>
      <button class="primary" onclick={save} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </button>
      {#if saved}<span class="ok" style="margin-left:12px">Saved.</span>{/if}
    </div>

    <div class="update-row">
      <div class="version">ValoRecon {settings.version} · Not affiliated with Riot Games</div>
      {#if updateStatus === "idle"}
        <button class="ghost sm" onclick={checkUpdate}>Check for updates</button>
      {:else if updateStatus === "checking"}
        <span class="muted" style="font-size:12px">Checking…</span>
      {:else if updateStatus === "uptodate"}
        <span class="ok" style="font-size:12px">Up to date</span>
      {:else if updateStatus === "available"}
        <button class="primary sm" onclick={installUpdate}>Update to v{updateVersion}</button>
      {:else if updateStatus === "installing"}
        <span class="muted" style="font-size:12px">Downloading…</span>
      {:else if updateStatus === "error"}
        <span class="bad" style="font-size:12px">Check failed</span>
      {/if}
    </div>
  </div>
{:else}
  <div class="hint">Loading settings…</div>
{/if}

<style>
.mhead { display: flex; align-items: center; gap: 12px; margin: 0 0 16px; }
.mhead h2 { font-size: 17px; margin: 0; font-weight: 900; letter-spacing: .4px; text-transform: uppercase; border-left: 3px solid var(--accent); padding-left: 10px; }
.form { max-width: 460px; display: flex; flex-direction: column; gap: 15px; }
.form label { font-size: 12px; color: var(--muted); display: block; margin-bottom: 7px; font-weight: 800; text-transform: uppercase; letter-spacing: .3px; }
.update-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 8px; }
.version { color: var(--dim); font-size: 11px; }
.toggle-row { display: flex; align-items: center; gap: 10px; }
.toggle-row input[type="checkbox"] { width: 16px; height: 16px; accent-color: var(--accent); }
.toggle-row label { font-size: 12px; color: var(--muted); font-weight: 800; text-transform: uppercase; letter-spacing: .3px; margin: 0; }
</style>
