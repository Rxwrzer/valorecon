<svelte:options runes={true} />
<script lang="ts">
  import { api, type Profile, type ProfileDeep, type PlayerStats } from "$lib/api";

  let loading = $state(true);
  let profile = $state<Profile | null>(null);
  let error = $state("");

  // Deep pull state
  let pullRunning = $state(false);
  let pullMsg = $state("");
  let pullMsgClass = $state("pullmsg muted");
  let pullPct = $state<number | null>(null);
  let pullCountInput = $state(50);
  let pullSource = $state<"riot" | "henrik">("riot");
  let hasKey = $state(false);
  let pollTimer: ReturnType<typeof setTimeout> | null = null;

  async function load() {
    loading = true;
    error = "";
    try {
      const s = await api.getSettings();
      pullSource = s.pull_source;
      hasKey = s.has_key;
      pullCountInput = s.profile_pull_target ?? 50;
    } catch {}
    try {
      const p = await api.refreshProfile();
      if ("error" in p && p.error) { error = p.error; }
      else { profile = p as Profile; }
    } catch (e) { error = String(e); }
    loading = false;
  }

  load();

  function sparkline(h: Profile["history"]) {
    if (!h.length) return "";
    const W = 680, H = 180, pad = 24;
    const ys = h.map(x => x.elo);
    const minY = Math.min(...ys) - 10, maxY = Math.max(...ys) + 10;
    const X = (i: number) => pad + (i / Math.max(1, h.length - 1)) * (W - 2 * pad);
    const Y = (v: number) => H - pad - ((v - minY) / Math.max(1, maxY - minY)) * (H - 2 * pad);
    const pts = h.map((x, i) => `${X(i).toFixed(1)},${Y(x.elo).toFixed(1)}`).join(" ");
    const area = `${pad},${H - pad} ${pts} ${W - pad},${H - pad}`;
    const dots = h.map((x, i) =>
      `<circle cx="${X(i).toFixed(1)}" cy="${Y(x.elo).toFixed(1)}" r="3" fill="${x.rr_change >= 0 ? '#4ade80' : '#f87171'}"><title>${x.tier_name} · ${x.rr_change >= 0 ? '+' : ''}${x.rr_change} RR · ${x.map_name}</title></circle>`
    ).join("");
    return `<svg viewBox="0 0 ${W} ${H}" width="100%" height="${H}">
      <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#ff4655" stop-opacity=".22"/>
        <stop offset="1" stop-color="#ff4655" stop-opacity="0"/></linearGradient></defs>
      <polygon points="${area}" fill="url(#g)"/>
      <polyline fill="none" stroke="#ff4655" stroke-width="2" points="${pts}"/>${dots}</svg>`;
  }

  function statVal(v: number | null, suffix = "") { return v != null ? v + suffix : "—"; }
  function statCls(v: number | null, good: number, bad: number) {
    if (v == null) return "muted";
    return v >= good ? "g" : v < bad ? "b" : "";
  }

  async function startPull(max: boolean) {
    const n = max ? 0 : Math.max(1, pullCountInput);
    pullRunning = true;
    pullMsg = ""; pullMsgClass = "pullmsg";
    try {
      const r = await api.profilePullStart(n, max);
      if (r.error) { pullMsgClass = "pullmsg b"; pullMsg = r.error; pullRunning = false; return; }
    } catch { pullRunning = false; return; }
    schedulePollPull();
  }

  async function stopPull() { try { await api.profilePullCancel(); } catch {} }

  function schedulePollPull() {
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = setTimeout(pollPull, 800);
  }

  async function pollPull() {
    let st;
    try { st = await api.profilePullStatus(); } catch { schedulePollPull(); return; }
    pullRunning = st.running;
    if (st.running) {
      const of = st.want_max ? "" : ` / ${st.target}`;
      pullMsgClass = "pullmsg";
      pullMsg = st.bucket_full_in > 0
        ? `Pulled ${st.added}${of} · rate limit — resuming in ${st.bucket_full_in}s`
        : `Pulled ${st.added}${of} · fetching…`;
      pullPct = st.want_max ? null : Math.min(100, Math.round(100 * st.added / Math.max(1, st.target)));
    } else {
      pullPct = null;
      if (st.error) { pullMsgClass = "pullmsg b"; pullMsg = st.error; }
      else if (st.done) {
        pullMsgClass = "pullmsg g";
        pullMsg = `Done — ${st.total_games} stored.`;
        if (st.history_end && st.history_seen)
          pullMsg += ` Riot's API currently exposes ~${st.history_seen} recent comp games.`;
      }
    }
    // Refresh deep stats
    try {
      const d = await api.profileStats();
      if (profile && !d.error) {
        profile = { ...profile, deep: d as ProfileDeep };
      }
    } catch {}
    if (st.running) schedulePollPull();
  }

  async function setSrc(src: "riot" | "henrik") {
    pullSource = src;
    try { await api.saveSettings({ pull_source: src }); } catch {}
  }
</script>

<div class="mhead">
  <h2>Your Rank</h2>
  {#if profile?.name}<span class="sub">· {profile.name}</span>{/if}
  <span class="spacer"></span>
  <button class="primary" onclick={load} disabled={loading}>
    {loading ? "Loading…" : "Refresh"}
  </button>
</div>

{#if loading}
  <div class="hint"><div class="big">Loading profile…</div></div>
{:else if error}
  <div class="hint"><div class="big">Can't load profile</div>{error}</div>
{:else if profile}
  <!-- Rank cards -->
  <div class="cards">
    <div class="card">
      <div class="k">Current rank</div>
      <div class="v">
        {#if profile.current.tier_icon}<img src={profile.current.tier_icon} alt="" />{/if}
        <div>{profile.current.tier_name}<br><small>{profile.current.tier > 0 ? profile.current.rr + " RR" : "Unranked"}</small></div>
      </div>
    </div>
    <div class="card">
      <div class="k">Peak rank</div>
      <div class="v">
        {#if profile.peak.tier_icon}<img src={profile.peak.tier_icon} alt="" />{/if}
        <div>{profile.peak.tier_name}<br><small>{profile.peak.tier > 0 ? "recent best" : "—"}</small></div>
      </div>
    </div>
  </div>

  <!-- Deep stats panel -->
  <div class="graph deep">
    <div class="dhead">
      <div class="k muted">Deep Stats — your competitive averages</div>
      <span class="dcount">{profile.deep?.games_total ?? 0} games stored</span>
    </div>
    <div class="deepwrap">
      {#snippet deepBlock(title: string, agg: PlayerStats | null, games: number)}
        <div class="deepcol">
          <div class="dch">
            <span>{title}</span>
            <span class="dgames">{games} games</span>
          </div>
          {#if !agg}
            <div class="dempty">No games {title === "This Act" ? "this act" : "pulled"} yet.</div>
          {:else}
            <div class="dgrid">
              <div class="dstat"><div class="dv {statCls(agg.winrate,55,45)}">{statVal(agg.winrate,"%")}</div><div class="dl">Win rate</div></div>
              <div class="dstat"><div class="dv {statCls(agg.kda,1.3,1.0)}">{agg.kda?.toFixed(2) ?? "—"}</div><div class="dl">KDA</div></div>
              <div class="dstat"><div class="dv {statCls(agg.acs,250,150)}">{statVal(agg.acs)}</div><div class="dl">ACS</div></div>
              <div class="dstat"><div class="dv {statCls(agg.adr,140,100)}">{statVal(agg.adr)}</div><div class="dl">ADR</div></div>
              <div class="dstat"><div class="dv {statCls(agg.hs,25,0)}">{statVal(agg.hs,"%")}</div><div class="dl">HS%</div></div>
              <div class="dstat"><div class="dv">{agg.avg_k} / {agg.avg_d} / {agg.avg_a}</div><div class="dl">Avg K/D/A</div></div>
            </div>
          {/if}
        </div>
      {/snippet}
      {@render deepBlock("Lifetime", profile.deep?.lifetime ?? null, profile.deep?.games_total ?? 0)}
      {@render deepBlock("This Act", profile.deep?.act ?? null, profile.deep?.games_act ?? 0)}
    </div>

    <!-- Source toggle -->
    <div class="srcrow">
      <span class="plbl">Source</span>
      <div class="srctoggle">
        <button class={pullSource === "riot" ? "active" : ""} onclick={() => setSrc("riot")}>Riot API</button>
        <button class={pullSource === "henrik" ? "active" : ""} onclick={() => setSrc("henrik")}>HenrikDev</button>
      </div>
      <span class="srchint {pullSource === 'henrik' && !hasKey ? 'warn2' : ''}">
        {pullSource === "henrik" && !hasKey ? "Needs a HenrikDev key (Settings)."
         : pullSource === "henrik" ? "Deeper history from HenrikDev's archive."
         : "Riot client API (recent games)."}
      </span>
    </div>

    <!-- Pull controls -->
    <div class="pullbar">
      <div class="pullgrp">
        <span class="plbl">Pull</span>
        <input type="number" min="1" max="5000" bind:value={pullCountInput} style="width:84px;padding:8px 10px"/>
        <span class="plbl">games</span>
        <button class="primary" onclick={() => startPull(false)} disabled={pullRunning}>Pull</button>
        <button class="ghost" onclick={() => startPull(true)} disabled={pullRunning}>Max</button>
        {#if pullRunning}
          <button class="ghost warn2" onclick={stopPull}>Stop</button>
        {/if}
      </div>
      <span class={pullMsgClass}>{pullMsg}</span>
      <span class="spacer"></span>
      <div class="pullgrp">
        <input type="number" min="1" value="20" style="width:70px;padding:8px 10px" id="delN" />
        <button class="ghost danger" onclick={async () => {
          const n = parseInt((document.getElementById('delN') as HTMLInputElement)?.value || '0');
          if (n <= 0) return;
          await api.profileDeleteOldest(n);
          load();
        }}>Delete oldest</button>
      </div>
    </div>

    {#if pullRunning}
      <div class="pullprog">
        <div class="pullbar-fill {pullPct == null ? 'indet' : ''}"
             style="width:{pullPct == null ? 40 : pullPct}%"></div>
      </div>
    {/if}

    <div class="pullnote">
      Riot's API only serves your recent competitive games, so a one-time pull can't reach your whole history.
      Every game you play is archived permanently, so your Lifetime sample keeps growing over time.
    </div>
  </div>

  <!-- Recent games -->
  <details class="recent">
    <summary>Recent competitive games <span>{profile.history.length}</span></summary>
    <div class="recent-body">
      <div class="graph">
        {@html sparkline(profile.history)}
      </div>
      <div class="glist">
        {#each [...profile.history].reverse() as h}
          <div class="grow">
            <div>
              {h.map_name || h.tier_name}
              {#if h.agent}<span class="muted">· {h.agent}</span>{/if}
            </div>
            <div class="muted">
              {#if h.kills >= 0}{h.kills}/{h.deaths}/{h.assists} · {h.hs}% HS{:else}—{/if}
            </div>
            <div class="muted">{h.rr_after} RR</div>
            <div class="delta {h.rr_change >= 0 ? 'up' : 'down'}">{h.rr_change >= 0 ? '+' : ''}{h.rr_change}</div>
          </div>
        {:else}
          <div class="dempty">No competitive games found this act.</div>
        {/each}
      </div>
    </div>
  </details>
{/if}

<style>
.mhead { display: flex; align-items: center; gap: 12px; margin: 0 0 16px; }
.mhead h2 { font-size: 17px; margin: 0; font-weight: 900; letter-spacing: .4px; text-transform: uppercase; border-left: 3px solid var(--accent); padding-left: 10px; }
.mhead .sub { color: var(--muted); font-size: 12.5px; font-weight: 700; }
.spacer { flex: 1; }

.cards { display: flex; gap: 13px; flex-wrap: wrap; margin-bottom: 18px; }
.card { position: relative; background: linear-gradient(180deg,var(--panel2),var(--panel)); border: 1px solid var(--line); border-radius: 12px; padding: 16px 18px; min-width: 152px; overflow: hidden; }
.card::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: var(--accent); }
.card .k { color: var(--muted); font-size: 11px; margin-bottom: 9px; font-weight: 800; text-transform: uppercase; letter-spacing: .6px; }
.card .v { display: flex; align-items: center; gap: 11px; font-size: 22px; font-weight: 900; }
.card .v img { width: 44px; height: 44px; }
.card small { color: var(--muted); font-weight: 700; font-size: 12px; }

.graph { background: linear-gradient(180deg,var(--panel2),var(--panel)); border: 1px solid var(--line); border-radius: 12px; padding: 16px 18px; margin-bottom: 0; }
.deep { margin-top: 16px; }
.dhead { display: flex; align-items: center; gap: 10px; margin-bottom: 12px; }
.dcount { margin-left: auto; color: var(--dim); font-size: 11px; font-weight: 800; text-transform: uppercase; letter-spacing: .5px; }
.deepwrap { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.deepcol { background: var(--panel); border: 1px solid var(--line); border-radius: 11px; padding: 14px 15px; }
.dch { display: flex; align-items: center; justify-content: space-between; margin-bottom: 11px; font-weight: 900; text-transform: uppercase; letter-spacing: .5px; font-size: 12.5px; }
.dgames { color: var(--dim); font-size: 10px; font-weight: 800; letter-spacing: .4px; }
.dgrid { display: grid; grid-template-columns: repeat(3,1fr); gap: 10px; }
.dstat { background: var(--panel2); border: 1px solid var(--line); border-radius: 9px; padding: 9px 6px; text-align: center; }
.dv { font-weight: 900; font-size: 17px; letter-spacing: .2px; }
.dl { font-size: 8.5px; color: var(--dim); text-transform: uppercase; letter-spacing: .6px; margin-top: 3px; font-weight: 800; }
.dempty { color: var(--muted); text-align: center; padding: 22px 0; font-size: 13px; }

.srcrow { display: flex; align-items: center; gap: 12px; margin-top: 15px; flex-wrap: wrap; }
.plbl { color: var(--muted); font-size: 11px; font-weight: 800; text-transform: uppercase; letter-spacing: .4px; }
.srctoggle { display: inline-flex; background: var(--panel3); border: 1px solid var(--line2); border-radius: 9px; padding: 3px; gap: 3px; }
.srctoggle button { background: none; border: 0; color: var(--muted); font: inherit; font-weight: 800; font-size: 12px; text-transform: uppercase; letter-spacing: .3px; padding: 6px 15px; border-radius: 7px; cursor: pointer; transition: .15s; }
.srctoggle button:hover { color: var(--text); }
.srctoggle button.active { background: var(--accent); color: #fff; box-shadow: 0 0 12px -3px var(--accent); }
.srchint { font-size: 11px; font-weight: 700; color: var(--dim); }

.pullbar { display: flex; align-items: center; gap: 12px; margin-top: 15px; flex-wrap: wrap; }
.pullgrp { display: flex; align-items: center; gap: 8px; }
.pullmsg { font-size: 12.5px; font-weight: 700; }
.pullprog { display: block; height: 6px; background: var(--panel3); border-radius: 6px; margin-top: 12px; overflow: hidden; }
.pullbar-fill { height: 100%; background: linear-gradient(90deg,var(--accent),var(--accent2)); border-radius: 6px; transition: width .3s; }
.pullbar-fill.indet { width: 40% !important; animation: indet 1.1s ease-in-out infinite; }
@keyframes indet { 0%{margin-left:-40%} 100%{margin-left:100%} }
.pullnote { color: var(--dim); font-size: 11px; margin-top: 11px; line-height: 1.45; max-width: 640px; }

details.recent { background: linear-gradient(180deg,var(--panel2),var(--panel)); border: 1px solid var(--line); border-radius: 12px; margin-top: 16px; overflow: hidden; }
details.recent > summary { cursor: pointer; list-style: none; padding: 14px 18px; font-weight: 900; text-transform: uppercase; letter-spacing: .4px; font-size: 13px; display: flex; align-items: center; gap: 9px; }
details.recent > summary::-webkit-details-marker { display: none; }
details.recent > summary > span { color: var(--dim); font-size: 11px; font-weight: 800; background: var(--panel3); border-radius: 20px; padding: 1px 9px; }
details.recent > summary::after { content: "\25BE"; margin-left: auto; color: var(--muted); transition: transform .2s; }
details.recent[open] > summary::after { transform: rotate(180deg); }
.recent-body { padding: 2px 16px 14px; }
.recent-body .graph { border: 0; background: transparent; padding: 0 0 6px; }
.glist { margin-top: 14px; display: flex; flex-direction: column; gap: 6px; }
.grow { display: grid; grid-template-columns: 1fr 150px 90px 66px; gap: 10px; align-items: center; background: var(--panel); border: 1px solid var(--line); border-left: 3px solid var(--line2); border-radius: 8px; padding: 9px 13px; font-size: 13px; }
.delta { font-weight: 900; text-align: right; }
</style>
