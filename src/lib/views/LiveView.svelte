<svelte:options runes={true} />
<script lang="ts">
  import { api, type AppState, type MatchPlayer } from "$lib/api";
  import RankBadge from "$lib/components/RankBadge.svelte";
  import StatCell from "$lib/components/StatCell.svelte";
  const PARTY_COLORS = ['#5b8cff','#ff9f43','#a66bff','#2dd4bf','#f472b6'];
  function partyColor(group: number): string {
    return group > 0 ? PARTY_COLORS[(group - 1) % PARTY_COLORS.length] : '';
  }
  import Toast from "$lib/components/Toast.svelte";

  let { appState } = $props<{ appState: AppState }>();

  let refreshing = $state(false);
  let toast: Toast | null = $state(null);

  const teams = $derived.by(() => {
    const map: Record<string, MatchPlayer[]> = {};
    for (const p of appState.players) {
      (map[p.team] ??= []).push(p);
    }
    return map;
  });

  const teamOrder = $derived.by(() => {
    return Object.keys(teams).sort((a, b) => {
      const sa = teams[a].some((p) => p.is_self);
      const sb = teams[b].some((p) => p.is_self);
      return sa ? -1 : sb ? 1 : 0;
    });
  });

  const teamColors: Record<string, string> = { Blue: "var(--blue)", Red: "var(--red)" };

  async function refreshNow() {
    if (refreshing) return;
    refreshing = true;
    try { await api.refreshNow(); } catch {}
    await new Promise(r => setTimeout(r, 600));
    refreshing = false;
  }

  function copyName(name: string, el: HTMLElement) {
    if (!name) return;
    navigator.clipboard?.writeText(name).catch(() => {
      const ta = document.createElement("textarea");
      ta.value = name;
      ta.style.cssText = "position:fixed;top:-1000px;opacity:0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    });
    el.classList.add("copied");
    setTimeout(() => el.classList.remove("copied"), 900);
    toast?.show("Copied " + name);
  }

  const GRID = "42px 188px 150px 112px repeat(5,minmax(0,1fr))";

  const statsLabel = $derived.by(() => {
    const counts = appState.players
      .filter((p: MatchPlayer) => p.stats && !p.stats_pending)
      .map((p: MatchPlayer) => p.stats!.games);
    if (!counts.length) return "stats loading…";
    const max = Math.max(...counts);
    return `stats = up to ${max} games`;
  });
</script>

<Toast bind:this={toast} />

<div class="liverow">
  <span class="livehint">
    {#if !appState.connected}Waiting for VALORANT — updates automatically
    {:else if appState.phase === "ingame"}In game — updates automatically
    {:else if appState.phase === "pregame"}Agent select — updates automatically
    {:else}In menus — updates automatically{/if}
  </span>
  <button class="ghost sm" onclick={refreshNow} disabled={refreshing}>
    {refreshing ? "↻ Checking…" : "↻ Refresh now"}
  </button>
</div>

{#if !appState.connected}
  <div class="hint">
    <div class="ico">⊹</div>
    <div class="big">Waiting for VALORANT</div>
    Launch the game — this updates automatically.
  </div>
{:else if appState.phase === "menus" || !appState.players.length}
  <div class="hint">
    <div class="ico">🎯</div>
    <div class="big">You're in the menus</div>
    Ranks and stats appear the moment you enter agent select or a match.
  </div>
{:else}
  <!-- Map hero banner -->
  {#if appState.map_image}
    <div class="hero">
      <div class="hero-bg" style="background-image:url('{appState.map_image}')"></div>
      <div class="hero-ov"></div>
      <div class="hero-in">
        <div>
          <div class="hero-map">{appState.map}</div>
          <h2>{appState.phase === "pregame" ? "Agent Select" : "Live Match"}</h2>
        </div>
        <span class="hero-note">{statsLabel}</span>
      </div>
      <span class="live-badge">{appState.phase === "pregame" ? "Picking" : "Live"}</span>
    </div>
  {:else}
    <div class="mhead">
      <h2>{appState.phase === "pregame" ? "Agent Select" : "Live Match"}</h2>
      <span class="sub">{appState.map}</span>
    </div>
  {/if}

  <!-- Column headers -->
  <div class="cols" style="--grid:{GRID}">
    <span></span><span>Player</span><span>Current</span><span>Peak</span>
    <span class="r">KDA</span><span class="r">ACS</span>
    <span class="r">ADR</span><span class="r">HS%</span><span class="r">WR</span>
  </div>

  <!-- Teams -->
  {#each teamOrder as tid}
    {@const teamPlayers = teams[tid]}
    {@const isSelfTeam = teamPlayers.some(p => p.is_self)}
    {@const color = teamColors[tid] ?? "var(--muted)"}
    <div class="team">
      <div class="team-h">
        <span style="color:{color}">
          {isSelfTeam ? "Your Team" : tid === "Red" || tid === "Blue" ? "Enemy Team" : "Team"}
        </span>
        <span class="bar" style="background:{color}"></span>
      </div>
      <div class="rows">
        {#each teamPlayers as p}
          <div
            class="row {p.is_self ? 'self' : ''} {p.party_group > 0 ? 'in-party' : ''}"
            style="--tier:{p.current.tier_color};--agent:{p.agent_color || p.current.tier_color};--grid:{GRID};--party:{partyColor(p.party_group)}"
          >
            <!-- Agent icon -->
            <img class="agent" src={p.agent_icon} alt={p.agent_name}
                 onerror={(e) => (e.currentTarget as HTMLElement).style.visibility = 'hidden'} />

            <!-- Name + meta -->
            <div class="pl">
              <div class="pname">
                {#if p.incognito && !p.is_self}
                  <span class="hidden">{p.agent_name || "Streamer Mode"}</span>
                {:else if p.name}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span class="copyname" title="Click to copy"
                        onclick={(e) => copyName(p.name, e.currentTarget as HTMLElement)}>
                    {p.name}
                  </span>
                {:else}
                  —
                {/if}
                {#if p.is_self}<span class="tag">YOU</span>{/if}
              </div>
              <div class="pmeta">
                {p.agent_name || ""}
                {#if !p.hide_level && p.account_level} · Lv {p.account_level}{/if}
              </div>
            </div>

            <!-- Current rank -->
            <RankBadge rank={p.current} pending={p.pending} />

            <!-- Peak rank -->
            {#if p.pending}
              <span class="peak muted">…</span>
            {:else if p.peak.tier > 0}
              <RankBadge rank={p.peak} size="small" />
            {:else}
              <span class="peak muted">—</span>
            {/if}

            <!-- Stats -->
            <StatCell stats={p.stats} pending={p.stats_pending} />
          </div>
        {/each}
      </div>
    </div>
  {/each}
{/if}

<style>
.liverow {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 0 14px;
}
.livehint {
  color: var(--dim);
  font-size: 11px;
  font-weight: 800;
  text-transform: uppercase;
  letter-spacing: .4px;
  flex: 1;
}

/* Map hero */
.hero {
  position: relative;
  min-height: 106px;
  border-radius: 14px;
  overflow: hidden;
  margin: 0 0 14px;
  border: 1px solid var(--line);
  border-left: 4px solid var(--accent);
}
.hero-bg { position: absolute; inset: 0; background-size: cover; background-position: center 40%; }
.hero-ov {
  position: absolute; inset: 0;
  background:
    linear-gradient(90deg, rgba(11,12,17,.95), rgba(11,12,17,.55) 52%, rgba(11,12,17,.16)),
    linear-gradient(0deg, rgba(11,12,17,.92), transparent 72%);
}
.hero-in {
  position: relative;
  min-height: 106px;
  display: flex;
  align-items: flex-end;
  gap: 12px;
  padding: 14px 18px;
}
.hero-in h2 {
  font-size: 24px;
  margin: 2px 0 0;
  font-weight: 900;
  text-transform: uppercase;
  letter-spacing: .6px;
  text-shadow: 0 2px 14px rgba(0,0,0,.85);
}
.hero-map {
  color: var(--accent);
  font-weight: 900;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 2px;
  text-shadow: 0 1px 8px rgba(0,0,0,.8);
}
.hero-note {
  margin-left: auto;
  color: #cdd2dd;
  font-size: 10.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .4px;
  background: rgba(0,0,0,.4);
  padding: 3px 9px;
  border-radius: 6px;
}
.live-badge {
  position: absolute;
  top: 12px;
  right: 14px;
  background: var(--accent);
  color: #fff;
  font-weight: 900;
  font-size: 10px;
  letter-spacing: 1px;
  padding: 4px 11px 4px 9px;
  border-radius: 6px;
  text-transform: uppercase;
  box-shadow: 0 0 16px -3px var(--accent);
  display: flex;
  align-items: center;
  gap: 6px;
}
.live-badge::before {
  content: "";
  width: 6px; height: 6px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 0 6px #fff;
}

.mhead { display: flex; align-items: center; gap: 12px; margin: 0 0 16px; }
.mhead h2 {
  font-size: 17px; margin: 0; font-weight: 900; letter-spacing: .4px;
  text-transform: uppercase; border-left: 3px solid var(--accent); padding-left: 10px;
}
.mhead .sub { color: var(--muted); font-size: 12.5px; font-weight: 700; }

/* Column headers */
.cols {
  display: grid;
  grid-template-columns: var(--grid);
  gap: 12px;
  padding: 0 16px 8px;
  color: var(--dim);
  font-size: 10px;
  font-weight: 800;
  text-transform: uppercase;
  letter-spacing: .8px;
}
.cols .r { text-align: center; }

/* Team */
.team { margin-bottom: 18px; }
.team-h {
  display: flex;
  align-items: center;
  gap: 11px;
  margin: 2px 0 9px;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: .9px;
  font-weight: 900;
}
.team-h .bar { height: 3px; flex: 1; border-radius: 2px; box-shadow: 0 0 10px -1px currentColor; }
.rows { display: flex; flex-direction: column; gap: 7px; }

/* Player row */
.row {
  display: grid;
  grid-template-columns: var(--grid);
  gap: 12px;
  align-items: center;
  position: relative;
  background: linear-gradient(90deg, color-mix(in srgb, var(--agent,#4b5160) 10%, var(--panel)), var(--panel) 44%);
  border: 1px solid var(--line);
  border-left: 4px solid var(--line2, #2a2d3a);
  border-radius: 10px;
  padding: 9px 14px;
  transition: transform .12s, border-color .15s, box-shadow .15s;
}
.row.in-party {
  border-left-color: var(--party);
}
.row:hover {
  transform: translateX(2px);
  border-color: color-mix(in srgb, var(--agent) 45%, var(--line2));
}
.row.self {
  background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 18%, var(--panel)), var(--panel) 50%);
  border-left-color: var(--accent);
}

.agent {
  width: 42px; height: 42px;
  border-radius: 9px;
  background: var(--panel3);
  object-fit: cover;
  box-shadow: inset 0 0 0 1px var(--line2), 0 0 0 2px color-mix(in srgb, var(--agent,#000) 45%, transparent);
}

.pl { min-width: 0; }
.pname {
  font-weight: 800;
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pname .hidden { color: var(--dim); font-style: italic; font-weight: 700; }
.pmeta {
  color: var(--muted);
  font-size: 11px;
  margin-top: 1px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-transform: uppercase;
  letter-spacing: .3px;
  font-weight: 600;
}
.tag {
  display: inline-block;
  font-size: 9px;
  color: #fff;
  background: var(--accent);
  border-radius: 4px;
  padding: 1px 5px;
  margin-left: 6px;
  vertical-align: middle;
  font-weight: 900;
  letter-spacing: .5px;
  box-shadow: 0 0 10px -2px var(--accent);
}
.peak {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
}

.copyname {
  cursor: pointer;
  border-bottom: 1px dashed transparent;
  border-radius: 3px;
  transition: color .12s, border-color .12s;
}
.copyname:hover { color: #fff; border-bottom-color: var(--accent); }
.copyname:active { color: var(--accent); }
:global(.copyname.copied) { color: var(--good); border-bottom-color: transparent; }
</style>
