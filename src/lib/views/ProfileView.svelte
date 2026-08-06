<svelte:options runes={true} />
<script lang="ts">
  import { api, type Profile, type ProfileDeep, type PlayerStats, type RRHistoryPoint } from "$lib/api";

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
    loading = true; error = "";
    try {
      const s = await api.getSettings();
      pullSource = s.pull_source; hasKey = s.has_key;
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

  function agentColor(name: string): string {
    const AGENT_COLORS: Record<string, string> = {
      Jett:"#4d7cff", Reyna:"#b56ad0", Raze:"#f0883e", Phoenix:"#ff7a3d", Yoru:"#3f6fd6",
      Neon:"#3fa9ff", Iso:"#5aa9d6", Breach:"#c8623a", Sova:"#4a7fb5", Fade:"#5b5f8c",
      Skye:"#5fb37a", KAYO:"#4a5568", Gekko:"#8fbf4d", Killjoy:"#e9c65a", Cypher:"#c9d1dc",
      Chamber:"#d4af6a", Sage:"#3fb6a8", Viper:"#3fae6a", Omen:"#5a5f9c", Brimstone:"#c1502e",
      Astra:"#8b5c9e", Harbor:"#3f9b7c", Clove:"#d98fd0", Deadlock:"#6a8caf", Vyse:"#7a5fc0",
    };
    if (!name) return "#4b5160";
    const key = name.replace(/[^a-zA-Z]/g, "");
    if (AGENT_COLORS[key]) return AGENT_COLORS[key];
    let h = 0; for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) & 0xffff;
    return `hsl(${h % 360} 42% 52%)`;
  }
  const monogram = (n: string) => (n || "").replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase() || "—";
  function hideImg(e: Event) { (e.currentTarget as HTMLElement).style.display = "none"; }

  function statVal(v: number | null, suffix = "") { return v != null ? v + suffix : "—"; }
  function statCls(v: number | null, good: number, bad: number) {
    if (v == null) return "";
    return v >= good ? "g" : v < bad ? "b" : "";
  }

  // Form pips + RR area chart from history (oldest → newest).
  const chrono = $derived.by(() => (profile?.history ? [...profile.history].reverse() : []));
  const form = $derived.by(() => {
    const c = chrono.slice(-12);
    const wins = c.filter((h) => h.rr_change > 0).length;
    return { pips: c.map((h) => h.rr_change >= 0), wins, losses: c.length - wins, n: c.length };
  });
  const chart = $derived.by(() => {
    const h = chrono;
    if (h.length < 2) return null;
    const W = 1000, H = 150, pad = 8;
    const ys = h.map((x) => x.elo);
    const min = Math.min(...ys), max = Math.max(...ys), sp = (max - min) || 1;
    const X = (i: number) => pad + (i / (h.length - 1)) * (W - 2 * pad);
    const Y = (v: number) => pad + (1 - (v - min) / sp) * (H - 2 * pad);
    const line = h.map((x, i) => `${X(i).toFixed(1)},${Y(x.elo).toFixed(1)}`).join(" ");
    const area = `${pad},${H - pad} ${line} ${W - pad},${H - pad}`;
    const dots = h.map((x, i) => ({ x: X(i), y: Y(x.elo), up: x.rr_change >= 0 }));
    return { W, H, line, area, dots };
  });

  async function startPull(max: boolean) {
    const n = max ? 0 : Math.max(1, pullCountInput);
    pullRunning = true; pullMsg = ""; pullMsgClass = "pullmsg";
    try {
      const r = await api.profilePullStart(n, max);
      if (r.error) { pullMsgClass = "pullmsg b"; pullMsg = r.error; pullRunning = false; return; }
    } catch { pullRunning = false; return; }
    schedulePollPull();
  }
  async function stopPull() { try { await api.profilePullCancel(); } catch {} }
  function schedulePollPull() { if (pollTimer) clearTimeout(pollTimer); pollTimer = setTimeout(pollPull, 800); }
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
        if (st.history_end && st.history_seen) pullMsg += ` Riot exposes ~${st.history_seen} recent comp games.`;
      }
    }
    try { const d = await api.profileStats(); if (profile && !d.error) profile = { ...profile, deep: d as ProfileDeep }; } catch {}
    if (st.running) schedulePollPull();
  }
  async function setSrc(src: "riot" | "henrik") { pullSource = src; try { await api.saveSettings({ pull_source: src }); } catch {} }

  function rankColor(c?: string) { return c && c !== "" ? c : "#8b90a0"; }
</script>

<div class="mhead">
  <h2>Your Rank</h2>
  <span class="spacer"></span>
  <button class="primary" onclick={load} disabled={loading}>{loading ? "Loading…" : "Refresh"}</button>
</div>

{#if loading}
  <div class="hint"><div class="big">Loading profile…</div></div>
{:else if error}
  <div class="hint"><div class="big">Can't load profile</div>{error}</div>
{:else if profile}
  {@const tc = rankColor(profile.current.tier_color)}
  {@const pc = rankColor(profile.peak.tier_color)}

  <!-- HERO -->
  <div class="hero" style="--tier:{tc};--peak:{pc}">
    <div class="hero-badge">
      {#if profile.current.tier_icon}
        <img src={profile.current.tier_icon} alt="" onerror={hideImg} />
      {:else}<div class="card-fallback"></div>{/if}
    </div>
    <div class="hid">
      <div class="name">{profile.name || "You"}</div>
      <div class="sub">Competitive <span class="d"></span> {profile.current.tier > 0 ? "Ranked" : "Unranked"}</div>
      <div class="rankline">
        <div class="rankchip">
          {#if profile.current.tier_icon}<img class="ti" src={profile.current.tier_icon} alt="" />{/if}
          <div>
            <div class="t">{profile.current.tier_name}</div>
            {#if profile.current.tier > 0}<div class="rr">{profile.current.rr} RR</div>{/if}
          </div>
        </div>
        {#if profile.peak.tier > 0}
          <div class="peakchip">
            {#if profile.peak.tier_icon}<img class="ti sm" src={profile.peak.tier_icon} alt="" />{/if}
            <span>Peak <span class="pk">{profile.peak.tier_name}</span></span>
          </div>
        {/if}
      </div>
      {#if profile.current.tier > 0}
        <div class="rrbar-wrap">
          <div class="rrbar"><i style="width:{Math.max(4, Math.min(100, profile.current.rr))}%"></i></div>
          <div class="rrbar-lbl"><span>{profile.current.tier_name}</span><span>{profile.current.rr} / 100 RR</span></div>
        </div>
      {/if}
    </div>
    {#if form.n > 0}
      <div class="hero-form">
        <div class="lbl">Last {form.n} · Form</div>
        <div class="pips">
          {#each form.pips as up}<span class="pip {up ? 'w' : 'l'}">{up ? "W" : "L"}</span>{/each}
        </div>
        <div class="rec"><b>{form.wins}</b>W · <b>{form.losses}</b>L</div>
      </div>
    {/if}
  </div>

  <!-- DEEP STATS -->
  <div class="panel">
    <div class="ph">Deep Stats — competitive averages<span class="cnt">{profile.deep?.games_total ?? 0} games stored</span></div>
    <div class="deepwrap">
      {#snippet deepBlock(title: string, agg: PlayerStats | null, games: number)}
        <div class="deepcol">
          <div class="dch"><span>{title}</span><span class="g2">{games} games</span></div>
          {#if !agg}
            <div class="dempty">No games {title === "This Act" ? "this act" : "pulled"} yet.</div>
          {:else}
            <div class="dgrid">
              <div class="dstat"><div class="dv {statCls(agg.winrate,55,45)}">{statVal(agg.winrate,"%")}</div><div class="dl">Win rate</div></div>
              <div class="dstat"><div class="dv {statCls(agg.kda,1.3,1.0)}">{agg.kda?.toFixed(2) ?? "—"}</div><div class="dl">KDA</div></div>
              <div class="dstat"><div class="dv {statCls(agg.acs,250,150)}">{statVal(agg.acs)}</div><div class="dl">ACS</div></div>
              <div class="dstat"><div class="dv {statCls(agg.adr,140,100)}">{statVal(agg.adr)}</div><div class="dl">ADR</div></div>
              <div class="dstat"><div class="dv warm">{statVal(agg.hs,"%")}</div><div class="dl">HS%</div></div>
              <div class="dstat"><div class="dv">{agg.avg_k} / {agg.avg_d} / {agg.avg_a}</div><div class="dl">Avg K/D/A</div></div>
            </div>
          {/if}
        </div>
      {/snippet}
      {@render deepBlock("Lifetime", profile.deep?.lifetime ?? null, profile.deep?.games_total ?? 0)}
      {@render deepBlock("This Act", profile.deep?.act ?? null, profile.deep?.games_act ?? 0)}
    </div>

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

    <div class="pullbar">
      <div class="pullgrp">
        <span class="plbl">Pull</span>
        <input type="number" min="1" max="5000" bind:value={pullCountInput} style="width:84px;padding:8px 10px"/>
        <span class="plbl">games</span>
        <button class="primary" onclick={() => startPull(false)} disabled={pullRunning}>Pull</button>
        <button class="ghost" onclick={() => startPull(true)} disabled={pullRunning}>Max</button>
        {#if pullRunning}<button class="ghost warn2" onclick={stopPull}>Stop</button>{/if}
      </div>
      <span class={pullMsgClass}>{pullMsg}</span>
      <span class="spacer"></span>
      <div class="pullgrp">
        <input type="number" min="1" value="20" style="width:70px;padding:8px 10px" id="delN" />
        <button class="ghost danger" onclick={async () => {
          const n = parseInt((document.getElementById('delN') as HTMLInputElement)?.value || '0');
          if (n <= 0) return; await api.profileDeleteOldest(n); load();
        }}>Delete oldest</button>
      </div>
    </div>

    {#if pullRunning}
      <div class="pullprog"><div class="pullbar-fill {pullPct == null ? 'indet' : ''}" style="width:{pullPct == null ? 40 : pullPct}%"></div></div>
    {/if}
    <div class="pullnote">
      Riot's API only serves recent competitive games, so a one-time pull can't reach your whole history.
      Every game you play is archived permanently, so your Lifetime sample keeps growing.
    </div>
  </div>

  <!-- RR TREND -->
  {#if chart}
    <div class="panel">
      <div class="ph">RR Trend — recent competitive<span class="cnt">{chrono.length} games</span></div>
      <svg viewBox="0 0 {chart.W} {chart.H}" width="100%" height="150" preserveAspectRatio="none">
        <defs><linearGradient id="rg" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="var(--accent)" stop-opacity=".24"/>
          <stop offset="1" stop-color="var(--accent)" stop-opacity="0"/></linearGradient></defs>
        <polygon points={chart.area} fill="url(#rg)"/>
        <polyline points={chart.line} fill="none" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round"/>
        {#each chart.dots as dt, i}
          {#if i > 0}<circle cx={dt.x} cy={dt.y} r="2.5" fill={dt.up ? "var(--good)" : "var(--bad)"}/>{/if}
        {/each}
      </svg>
    </div>
  {/if}

  <!-- RECENT GAMES -->
  <div class="listhead"><h3>Recent Competitive</h3><span class="rule"></span></div>
  {#if !profile.history.length}
    <div class="dempty">No competitive games found this act.</div>
  {:else}
    <div class="matches">
      {#each [...profile.history].reverse() as h}
        {@const up = h.rr_change >= 0}
        {@const hasStats = h.kills >= 0}
        <div class="m {up ? 'win' : 'loss'}">
          <span class="stripe"></span>
          <span class="face" style="background:{agentColor(h.agent)}">
            {monogram(h.agent)}
            {#if h.agent_icon}<img src={h.agent_icon} alt="" onerror={hideImg} />{/if}
          </span>
          <div class="who"><div class="agn">{h.agent || h.map_name}</div><div class="mp">{h.agent ? h.map_name : h.tier_name}</div></div>
          <div class="result"><div class="rr2 mono">{h.rr_after} RR</div><div class="wl">{up ? "Gain" : "Loss"}</div></div>
          <div class="kda">
            {#if hasStats}
              <div class="nums mono"><b>{h.kills}</b> / {h.deaths} / {h.assists}</div><div class="kdr mono">HS {h.hs}%</div>
            {:else}<div class="nums muted">—</div>{/if}
          </div>
          <div class="acs"><div class="v2 {up ? 'g' : 'b'} mono">{up ? "+" : ""}{h.rr_change}</div><div class="l2">RR Δ</div></div>
          <div class="hs"><div class="v2 mono">{hasStats ? h.hs + "%" : "—"}</div><div class="l2">HS</div></div>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .mhead { display: flex; align-items: center; gap: 12px; margin: 0 0 16px; }
  .mhead h2 { font-size: 17px; margin: 0; font-weight: 900; letter-spacing: .4px; text-transform: uppercase; border-left: 3px solid var(--accent); padding-left: 10px; }
  .spacer { flex: 1; }
  .mono { font-variant-numeric: tabular-nums; }

  /* hero */
  .hero { position: relative; display: grid; grid-template-columns: auto 1fr auto; gap: 22px; align-items: center;
    background: linear-gradient(120deg, color-mix(in srgb, var(--tier) 14%, var(--panel2)), var(--panel) 46%);
    border: 1px solid var(--line); border-radius: 16px; padding: 20px 24px; overflow: hidden; margin-bottom: 14px; }
  .hero::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 4px; background: var(--tier); box-shadow: 0 0 24px 1px var(--tier); }
  .hero-badge { position: relative; width: 92px; height: 92px; border-radius: 16px; overflow: hidden; display: grid; place-items: center;
    background: radial-gradient(circle at 50% 35%, color-mix(in srgb, var(--tier) 26%, var(--panel3)), var(--panel3));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tier) 40%, transparent), 0 6px 18px -6px rgba(0,0,0,.6); }
  .hero-badge img { width: 66px; height: 66px; filter: drop-shadow(0 3px 10px rgba(0,0,0,.5)); }
  .card-fallback { width: 100%; height: 100%; }
  .hid .name { font-size: 26px; font-weight: 900; line-height: 1.1; }
  .hid .sub { color: var(--muted); font-weight: 700; font-size: 12px; margin-top: 6px; text-transform: uppercase; letter-spacing: .5px; display: flex; align-items: center; gap: 8px; }
  .hid .sub .d { width: 3px; height: 3px; border-radius: 50%; background: var(--dim); }
  .rankline { display: flex; align-items: center; gap: 12px; margin-top: 13px; flex-wrap: wrap; }
  .rankchip { display: flex; align-items: center; gap: 9px; background: color-mix(in srgb, var(--tier) 12%, transparent); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tier) 34%, transparent); border-radius: 10px; padding: 6px 12px 6px 8px; }
  .rankchip .ti { width: 26px; height: 26px; }
  .rankchip .t { font-weight: 900; font-size: 14px; color: var(--tier); text-transform: uppercase; letter-spacing: .3px; line-height: 1.1; }
  .rankchip .rr { color: var(--muted); font-size: 11px; font-weight: 800; }
  .peakchip { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 12px; font-weight: 700; }
  .peakchip .ti.sm { width: 20px; height: 20px; }
  .peakchip .pk { color: var(--peak); font-weight: 900; text-transform: uppercase; }
  .rrbar-wrap { margin-top: 14px; max-width: 340px; }
  .rrbar { height: 6px; border-radius: 4px; background: var(--panel3); overflow: hidden; box-shadow: inset 0 0 0 1px var(--line); }
  .rrbar > i { display: block; height: 100%; border-radius: 4px; background: linear-gradient(90deg, var(--tier), color-mix(in srgb, var(--tier) 60%, #fff)); }
  .rrbar-lbl { display: flex; justify-content: space-between; margin-top: 5px; font-size: 10px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .5px; }
  .hero-form { text-align: right; align-self: flex-start; }
  .hero-form .lbl { font-size: 10px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .7px; margin-bottom: 8px; }
  .pips { display: flex; gap: 5px; justify-content: flex-end; flex-wrap: wrap; }
  .pip { width: 16px; height: 16px; border-radius: 5px; display: grid; place-items: center; font-size: 9px; font-weight: 900; color: #0c0d12; }
  .pip.w { background: var(--good); } .pip.l { background: var(--bad); }
  .hero-form .rec { margin-top: 9px; font-size: 12px; font-weight: 800; color: var(--muted); }
  .hero-form .rec b { color: var(--text); }

  /* panels */
  .panel { background: linear-gradient(180deg, var(--panel2), var(--panel)); border: 1px solid var(--line); border-radius: 12px; padding: 16px 18px; margin-bottom: 14px; }
  .panel .ph { font-size: 11px; font-weight: 800; color: var(--muted); text-transform: uppercase; letter-spacing: .6px; margin-bottom: 12px; display: flex; align-items: center; gap: 10px; }
  .panel .ph .cnt { margin-left: auto; color: var(--dim); font-size: 10px; font-weight: 800; letter-spacing: .4px; }
  .deepwrap { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .deepcol { background: var(--panel); border: 1px solid var(--line); border-radius: 11px; padding: 14px 15px; }
  .dch { display: flex; justify-content: space-between; margin-bottom: 11px; font-weight: 900; text-transform: uppercase; letter-spacing: .5px; font-size: 12.5px; }
  .dch .g2 { color: var(--dim); font-size: 10px; }
  .dgrid { display: grid; grid-template-columns: repeat(3,1fr); gap: 10px; }
  .dstat { background: var(--panel2); border: 1px solid var(--line); border-radius: 9px; padding: 9px 6px; text-align: center; }
  .dv { font-weight: 900; font-size: 17px; } .dv.g { color: var(--good); } .dv.b { color: var(--bad); } .dv.warm { color: var(--warn); }
  .dl { font-size: 8.5px; color: var(--dim); text-transform: uppercase; letter-spacing: .6px; margin-top: 3px; font-weight: 800; }
  .dempty { color: var(--muted); text-align: center; padding: 22px 0; font-size: 13px; }

  .srcrow { display: flex; align-items: center; gap: 12px; margin-top: 15px; flex-wrap: wrap; }
  .plbl { color: var(--muted); font-size: 11px; font-weight: 800; text-transform: uppercase; letter-spacing: .4px; }
  .srctoggle { display: inline-flex; background: var(--panel3); border: 1px solid var(--line2); border-radius: 9px; padding: 3px; gap: 3px; }
  .srctoggle button { background: none; border: 0; color: var(--muted); font: inherit; font-weight: 800; font-size: 12px; text-transform: uppercase; letter-spacing: .3px; padding: 6px 15px; border-radius: 7px; cursor: pointer; transition: .15s; }
  .srctoggle button:hover { color: var(--text); }
  .srctoggle button.active { background: var(--accent); color: #fff; box-shadow: 0 0 12px -3px var(--accent); }
  .srchint { font-size: 11px; font-weight: 700; color: var(--dim); }
  .warn2 { color: var(--warn); }
  .pullbar { display: flex; align-items: center; gap: 12px; margin-top: 15px; flex-wrap: wrap; }
  .pullgrp { display: flex; align-items: center; gap: 8px; }
  .pullmsg { font-size: 12.5px; font-weight: 700; } .pullmsg.g { color: var(--good); } .pullmsg.b { color: var(--bad); }
  .pullprog { display: block; height: 6px; background: var(--panel3); border-radius: 6px; margin-top: 12px; overflow: hidden; }
  .pullbar-fill { height: 100%; background: linear-gradient(90deg,var(--accent),var(--accent2)); border-radius: 6px; transition: width .3s; }
  .pullbar-fill.indet { width: 40% !important; animation: indet 1.1s ease-in-out infinite; }
  @keyframes indet { 0%{margin-left:-40%} 100%{margin-left:100%} }
  .pullnote { color: var(--dim); font-size: 11px; margin-top: 11px; line-height: 1.45; max-width: 640px; }

  /* match list */
  .listhead { display: flex; align-items: center; gap: 10px; margin: 18px 0 10px; }
  .listhead h3 { font-size: 12px; margin: 0; font-weight: 900; letter-spacing: .8px; text-transform: uppercase; color: var(--muted); }
  .listhead .rule { flex: 1; height: 1px; background: var(--line); }
  .matches { display: flex; flex-direction: column; gap: 7px; }
  .m { display: grid; grid-template-columns: 4px 44px 1.4fr 96px 1.1fr 70px 66px; gap: 14px; align-items: center;
    background: linear-gradient(90deg, color-mix(in srgb, var(--rc) 8%, var(--panel)), var(--panel) 42%);
    border: 1px solid var(--line); border-radius: 11px; padding: 9px 15px 9px 0; transition: transform .12s, border-color .15s; }
  .m.win { --rc: var(--good); } .m.loss { --rc: var(--bad); }
  .m:hover { transform: translateX(2px); border-color: color-mix(in srgb, var(--rc) 40%, var(--line2)); }
  .m .stripe { align-self: stretch; border-radius: 11px 0 0 11px; background: var(--rc); margin: -9px 0; }
  .m .face { position: relative; width: 44px; height: 44px; border-radius: 9px; display: grid; place-items: center; font-weight: 900; font-size: 15px; color: #0c0d12; box-shadow: inset 0 0 0 1px rgba(255,255,255,.12); overflow: hidden; }
  .m .face img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .m .who { min-width: 0; }
  .m .who .agn { font-weight: 800; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .m .who .mp { font-size: 11px; color: var(--muted); font-weight: 600; text-transform: uppercase; letter-spacing: .3px; margin-top: 1px; }
  .m .result { text-align: center; }
  .m .result .rr2 { font-weight: 900; font-size: 15px; }
  .m .result .wl { font-size: 9.5px; font-weight: 800; letter-spacing: .6px; text-transform: uppercase; color: var(--dim); margin-top: 2px; }
  .m .kda .nums { font-size: 14px; font-weight: 700; } .m .kda .nums b { font-weight: 900; }
  .m .kda .kdr { font-size: 11px; color: var(--muted); font-weight: 700; margin-top: 1px; }
  .m .acs, .m .hs { text-align: center; }
  .m .acs .v2 { font-weight: 900; font-size: 15px; } .m .acs .v2.g { color: var(--good); } .m .acs .v2.b { color: var(--bad); }
  .m .hs .v2 { font-weight: 800; font-size: 14px; color: var(--muted); }
  .m .acs .l2, .m .hs .l2 { font-size: 8.5px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .6px; margin-top: 2px; }
</style>
