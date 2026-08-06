<svelte:options runes={true} />
<script lang="ts">
  import { api, type LookupResult } from "$lib/api";

  let riotId = $state("");
  let loading = $state(false);
  let result = $state<LookupResult | null>(null);

  async function doLookup() {
    const id = riotId.trim();
    if (!id) return;
    loading = true; result = null;
    try { result = await api.lookup(id); } catch { result = { error: "Bridge error." }; }
    loading = false;
  }

  // ── Agent visuals (colored monogram faces) ────────────────────────────────
  const AGENT_COLORS: Record<string, string> = {
    Jett:"#4d7cff", Reyna:"#b56ad0", Raze:"#f0883e", Phoenix:"#ff7a3d", Yoru:"#3f6fd6",
    Neon:"#3fa9ff", Iso:"#5aa9d6", Breach:"#c8623a", Sova:"#4a7fb5", Fade:"#5b5f8c",
    Skye:"#5fb37a", KAYO:"#4a5568", Gekko:"#8fbf4d", Killjoy:"#e9c65a", Cypher:"#c9d1dc",
    Chamber:"#d4af6a", Sage:"#3fb6a8", Viper:"#3fae6a", Omen:"#5a5f9c", Brimstone:"#c1502e",
    Astra:"#8b5c9e", Harbor:"#3f9b7c", Clove:"#d98fd0", Deadlock:"#6a8caf", Vyse:"#7a5fc0",
    Tejo:"#c25a4a", Waylay:"#e0a44a",
  };
  function agentColor(name: string): string {
    if (!name) return "#4b5160";
    const key = name.replace(/[^a-zA-Z]/g, "");
    if (AGENT_COLORS[key]) return AGENT_COLORS[key];
    let h = 0;
    for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) & 0xffff;
    return `hsl(${h % 360} 42% 52%)`;
  }
  function monogram(name: string): string {
    const letters = (name || "").replace(/[^a-zA-Z]/g, "");
    return letters.slice(0, 2).toUpperCase() || "—";
  }

  function cls(v: number | null, good: number, bad: number) {
    if (v == null) return "n";
    return v >= good ? "g" : v < bad ? "b" : "n";
  }

  // ── Aggregate over the returned matches ───────────────────────────────────
  const summary = $derived.by(() => {
    const gs = result?.matches;
    if (!gs?.length) return null;
    let k = 0, d = 0, a = 0, score = 0, rounds = 0, dmg = 0, hsW = 0, wW = 0, wins = 0, decided = 0;
    const agentCount: Record<string, number> = {};
    const agentIcon: Record<string, string> = {};
    for (const g of gs) {
      k += g.kills; d += g.deaths; a += g.assists;
      score += g.score || 0; rounds += g.rounds || 0; dmg += g.damage || 0;
      const w = g.rounds || 1;
      hsW += (g.hs || 0) * w; wW += w;
      if (g.won === true) { wins++; decided++; }
      else if (g.won === false) { decided++; }
      if (g.agent) {
        agentCount[g.agent] = (agentCount[g.agent] ?? 0) + 1;
        if (g.agent_icon) agentIcon[g.agent] = g.agent_icon;
      }
    }
    const n = gs.length;
    const topAgent = Object.entries(agentCount).sort((x, y) => y[1] - x[1])[0]?.[0] ?? "—";
    const topAgentGames = topAgent === "—" ? 0 : agentCount[topAgent];
    // oldest → newest for the form strip (matches arrive newest-first)
    const form = [...gs].reverse().map((g) => g.won);
    return {
      games: n,
      kda: d > 0 ? ((k + a) / d).toFixed(2) : (k + a).toFixed(2),
      avgK: (k / n).toFixed(1), avgD: (d / n).toFixed(1), avgA: (a / n).toFixed(1),
      acs: rounds > 0 ? Math.round(score / rounds) : null,
      adr: rounds > 0 ? Math.round(dmg / rounds) : null,
      hs: wW > 0 ? Math.round(hsW / wW) : Math.round(gs.reduce((s, g) => s + g.hs, 0) / n),
      wr: decided > 0 ? Math.round((wins / decided) * 100) : null,
      wins, losses: decided - wins, decided,
      topAgent, topAgentGames, topAgentIcon: agentIcon[topAgent] ?? "", form,
    };
  });

  function rankFallbackColor(c?: string) { return c && c !== "" ? c : "#8b90a0"; }
  function hideImg(e: Event) { (e.currentTarget as HTMLElement).style.display = "none"; }
</script>

<div class="mhead"><h2>Look Up a Player</h2></div>

<div class="searchbar">
  <input
    bind:value={riotId}
    placeholder="Name#TAG"
    autocomplete="off"
    onkeydown={(e) => e.key === "Enter" && doLookup()}
  />
  <button class="primary" onclick={doLookup} disabled={loading}>Search</button>
</div>

{#if !result && !loading}
  <div class="hint muted">Enter a Riot ID like <b>TenZ#0505</b>.</div>
{:else if loading}
  <div class="hint">Searching…</div>
{:else if result?.error}
  <div class="hint"><div class="big">{result.error}</div></div>
{:else if result?.account}
  {@const a = result.account!}
  {@const m = result.mmr!}
  {@const tc = rankFallbackColor(m?.current_tier_color)}
  {@const pc = rankFallbackColor(m?.peak_tier_color)}

  <!-- HERO -->
  <div class="hero" style="--tier:{tc};--peak-tier:{pc}">
    <div class="hero-badge">
      {#if a.card}
        <img src={a.card} alt="Player card"
             onerror={(e) => (e.currentTarget as HTMLElement).style.display='none'} />
      {:else}
        <div class="card-fallback"></div>
      {/if}
      <span class="lvl">{a.level}</span>
    </div>

    <div class="hero-id">
      <div class="name">{a.name}<span class="tag">#{a.tag}</span></div>
      <div class="sub">
        {a.region?.toUpperCase()} <span class="d"></span> Level {a.level} <span class="d"></span> Competitive
      </div>
      <div class="rankline">
        <div class="rankchip">
          {#if m?.current_tier_icon}
            <img class="ti" src={m.current_tier_icon} alt="" />
          {/if}
          <div>
            <div class="t">{m?.current_tier_name || "Unranked"}</div>
            {#if m?.current_rr != null && (m?.current_tier ?? 0) > 0}
              <div class="rr">{m.current_rr} RR</div>
            {/if}
          </div>
        </div>
        {#if m?.peak_tier_name}
          <div class="peakchip">
            {#if m?.peak_tier_icon}<img class="ti sm" src={m.peak_tier_icon} alt="" />{/if}
            <span>Peak <span class="pk">{m.peak_tier_name}</span></span>
          </div>
        {/if}
      </div>
      {#if m?.current_rr != null && (m?.current_tier ?? 0) > 0}
        <div class="rrbar-wrap">
          <div class="rrbar"><i style="width:{Math.max(4, Math.min(100, m.current_rr))}%"></i></div>
          <div class="rrbar-lbl"><span>{m.current_tier_name}</span><span>{m.current_rr} / 100 RR</span></div>
        </div>
      {/if}
    </div>

    {#if summary}
      <div class="hero-form">
        <div class="lbl">Last {summary.games} · Form</div>
        <div class="pips">
          {#each summary.form as w}
            <span class="pip {w === true ? 'w' : w === false ? 'l' : 'u'}">{w === true ? "W" : w === false ? "L" : "·"}</span>
          {/each}
        </div>
        {#if summary.wr != null}
          <div class="rec"><b>{summary.wins}</b>W · <b>{summary.losses}</b>L <span class="dim">· {summary.wr}%</span></div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- STAT TILES -->
  {#if summary}
    <div class="stats">
      <div class="tile">
        <div class="v n mono">{summary.kda}</div>
        <div class="k">KDA Ratio</div>
        <div class="sub mono">{summary.avgK} / {summary.avgD} / {summary.avgA}</div>
      </div>
      <div class="tile">
        <div class="v {cls(summary.acs, 250, 150)} mono">{summary.acs ?? "—"}</div>
        <div class="k">Avg ACS</div>
        <div class="sub">combat score / round</div>
      </div>
      <div class="tile">
        <div class="v n mono">{summary.adr ?? "—"}</div>
        <div class="k">Avg ADR</div>
        <div class="sub">damage / round</div>
      </div>
      <div class="tile">
        <div class="v warm mono">{summary.hs}%</div>
        <div class="k">Headshot %</div>
        <div class="sub">across {summary.games} games</div>
      </div>
      <div class="tile">
        <div class="v n mono">{summary.wr != null ? summary.wr + "%" : "—"}</div>
        <div class="k">Win Rate</div>
        {#if summary.wr != null}<div class="sub mono">{summary.wins}W · {summary.losses}L</div>{/if}
      </div>
      <div class="tile">
        <div class="k" style="margin-top:0;margin-bottom:9px">Most Played</div>
        <div class="agentchip">
          <span class="mono-face" style="background:{agentColor(summary.topAgent)}">
            {monogram(summary.topAgent)}
            {#if summary.topAgentIcon}<img src={summary.topAgentIcon} alt="" onerror={hideImg} />{/if}
          </span>
          <div>
            <div class="ag-name">{summary.topAgent}</div>
            <div class="sub" style="margin-top:2px">{summary.topAgentGames} games</div>
          </div>
        </div>
      </div>
    </div>
  {/if}

  <!-- MATCH LIST -->
  <div class="listhead"><h2>Recent Competitive</h2><span class="rule"></span></div>
  {#if !result.matches?.length}
    <div class="hint muted">No recent competitive matches.</div>
  {:else}
    <div class="matches">
      {#each result.matches! as g}
        {@const won = g.won === true}
        {@const lost = g.won === false}
        {@const macs = g.rounds > 0 ? Math.round(g.score / g.rounds) : null}
        <div class="m {won ? 'win' : lost ? 'loss' : 'tie'}">
          <span class="stripe"></span>
          <span class="face" style="background:{agentColor(g.agent)}">
            {monogram(g.agent)}
            {#if g.agent_icon}<img src={g.agent_icon} alt="" onerror={hideImg} />{/if}
          </span>
          <div class="who">
            <div class="agn">{g.agent || "—"}{#if g.mvp}<span class="mvp {g.mvp}" title={g.mvp === "match" ? "Match MVP" : "Team MVP"}>MVP</span>{/if}</div>
            <div class="mp">{g.map}</div>
          </div>
          <div class="result">
            <div class="rr2 mono">{g.my_rounds}–{g.enemy_rounds}</div>
            <div class="wl">{won ? "Victory" : lost ? "Defeat" : "—"}</div>
          </div>
          <div class="kda">
            <div class="nums mono"><b>{g.kills}</b> / {g.deaths} / {g.assists}</div>
            <div class="kdr mono">{g.kd} KD</div>
          </div>
          <div class="acs"><div class="v2 {cls(macs, 250, 150)} mono">{macs ?? "—"}</div><div class="l2">ACS</div></div>
          <div class="hs"><div class="v2 mono">{g.hs}%</div><div class="l2">HS</div></div>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .mhead { display: flex; align-items: center; gap: 12px; margin: 0 0 16px; }
  .mhead h2 { font-size: 17px; margin: 0; font-weight: 900; letter-spacing: .4px; text-transform: uppercase; border-left: 3px solid var(--accent); padding-left: 10px; }
  .searchbar { display: flex; gap: 9px; margin-bottom: 16px; max-width: 440px; }
  .mono { font-variant-numeric: tabular-nums; }
  .dim { color: var(--dim); }

  /* ── HERO ─────────────────────────────────────────────────── */
  .hero {
    position: relative; display: grid; grid-template-columns: auto 1fr auto;
    gap: 22px; align-items: center;
    background: linear-gradient(120deg, color-mix(in srgb, var(--tier) 14%, var(--panel2)), var(--panel) 46%);
    border: 1px solid var(--line); border-radius: 16px; padding: 20px 24px; overflow: hidden; margin-bottom: 14px;
  }
  .hero::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 4px; background: var(--tier); box-shadow: 0 0 24px 1px var(--tier); }
  .hero-badge { position: relative; width: 92px; height: 92px; border-radius: 16px; overflow: hidden;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tier) 40%, transparent), 0 6px 18px -6px rgba(0,0,0,.6); }
  .hero-badge img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .card-fallback { width: 100%; height: 100%; background: radial-gradient(circle at 50% 35%, color-mix(in srgb, var(--tier) 30%, var(--panel3)), var(--panel3)); }
  .hero-badge .lvl { position: absolute; left: 50%; bottom: 6px; transform: translateX(-50%);
    background: rgba(8,9,13,.86); color: #fff; font-weight: 900; font-size: 12px; padding: 2px 9px; border-radius: 7px; letter-spacing: .3px;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tier) 55%, transparent); }

  .hero-id { min-width: 0; }
  .hero-id .name { font-size: 26px; font-weight: 900; letter-spacing: .3px; line-height: 1.1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hero-id .name .tag { color: var(--muted); font-weight: 800; font-size: 18px; }
  .hero-id .sub { color: var(--muted); font-weight: 700; font-size: 12px; margin-top: 6px; text-transform: uppercase; letter-spacing: .5px; display: flex; align-items: center; gap: 8px; }
  .hero-id .sub .d { width: 3px; height: 3px; border-radius: 50%; background: var(--dim); }
  .rankline { display: flex; align-items: center; gap: 12px; margin-top: 13px; flex-wrap: wrap; }
  .rankchip { display: flex; align-items: center; gap: 9px; background: color-mix(in srgb, var(--tier) 12%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tier) 34%, transparent); border-radius: 10px; padding: 6px 12px 6px 8px; }
  .rankchip .ti { width: 26px; height: 26px; }
  .rankchip .t { font-weight: 900; font-size: 14px; color: var(--tier); text-transform: uppercase; letter-spacing: .3px; line-height: 1.1; }
  .rankchip .rr { color: var(--muted); font-size: 11px; font-weight: 800; }
  .peakchip { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 12px; font-weight: 700; }
  .peakchip .ti.sm { width: 20px; height: 20px; }
  .peakchip .pk { color: var(--peak-tier); font-weight: 900; text-transform: uppercase; letter-spacing: .3px; }
  .rrbar-wrap { margin-top: 14px; max-width: 340px; }
  .rrbar { height: 6px; border-radius: 4px; background: var(--panel3); overflow: hidden; box-shadow: inset 0 0 0 1px var(--line); }
  .rrbar > i { display: block; height: 100%; border-radius: 4px; background: linear-gradient(90deg, var(--tier), color-mix(in srgb, var(--tier) 60%, #fff)); }
  .rrbar-lbl { display: flex; justify-content: space-between; margin-top: 5px; font-size: 10px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .5px; }

  .hero-form { text-align: right; align-self: flex-start; }
  .hero-form .lbl { font-size: 10px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .7px; margin-bottom: 8px; }
  .pips { display: flex; gap: 5px; justify-content: flex-end; flex-wrap: wrap; }
  .pip { width: 16px; height: 16px; border-radius: 5px; display: grid; place-items: center; font-size: 9px; font-weight: 900; color: #0c0d12; }
  .pip.w { background: var(--good); } .pip.l { background: var(--bad); } .pip.u { background: var(--line2); color: var(--dim); }
  .hero-form .rec { margin-top: 9px; font-size: 12px; font-weight: 800; color: var(--muted); }
  .hero-form .rec b { color: var(--text); }

  /* ── STAT TILES ───────────────────────────────────────────── */
  .stats { display: grid; grid-template-columns: repeat(6, 1fr); gap: 10px; margin-bottom: 14px; }
  .tile { background: linear-gradient(180deg, var(--panel2), var(--panel)); border: 1px solid var(--line); border-radius: 12px; padding: 13px 14px 12px; overflow: hidden; }
  .tile .v { font-size: 24px; font-weight: 900; letter-spacing: .2px; line-height: 1; }
  .tile .k { font-size: 9px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .8px; margin-top: 7px; }
  .tile .sub { font-size: 10.5px; font-weight: 700; color: var(--muted); margin-top: 4px; }
  .v.g { color: var(--good); } .v.b { color: var(--bad); } .v.n { color: var(--text); } .v.warm { color: var(--warn); }
  .spark { margin-top: 8px; }
  .agentchip { display: flex; align-items: center; gap: 9px; }
  .agentchip .mono-face { position: relative; width: 30px; height: 30px; border-radius: 8px; display: grid; place-items: center; font-weight: 900; font-size: 12px; color: #0c0d12; box-shadow: inset 0 0 0 1px rgba(255,255,255,.14); overflow: hidden; }
  .agentchip .mono-face img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .agentchip .ag-name { font-weight: 900; font-size: 15px; }

  /* ── MATCH LIST ───────────────────────────────────────────── */
  .listhead { display: flex; align-items: center; gap: 10px; margin: 18px 0 10px; }
  .listhead h2 { font-size: 12px; margin: 0; font-weight: 900; letter-spacing: .8px; text-transform: uppercase; color: var(--muted); }
  .listhead .rule { flex: 1; height: 1px; background: var(--line); }
  .matches { display: flex; flex-direction: column; gap: 7px; }
  .m { display: grid; grid-template-columns: 4px 44px 1.4fr 96px 1.1fr 70px 66px; gap: 14px; align-items: center;
    background: linear-gradient(90deg, color-mix(in srgb, var(--rc) 8%, var(--panel)), var(--panel) 42%);
    border: 1px solid var(--line); border-radius: 11px; padding: 9px 15px 9px 0; transition: transform .12s, border-color .15s; }
  .m.win { --rc: var(--good); } .m.loss { --rc: var(--bad); } .m.tie { --rc: var(--line2); }
  .m:hover { transform: translateX(2px); border-color: color-mix(in srgb, var(--rc) 40%, var(--line2)); }
  .m .stripe { align-self: stretch; border-radius: 11px 0 0 11px; background: var(--rc); margin: -9px 0; }
  .m .face { position: relative; width: 44px; height: 44px; border-radius: 9px; display: grid; place-items: center; font-weight: 900; font-size: 15px; color: #0c0d12; box-shadow: inset 0 0 0 1px rgba(255,255,255,.12); overflow: hidden; }
  .m .face img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .m .who { min-width: 0; }
  .m .who .agn { font-weight: 800; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .m .who .mvp { display: inline-block; font-size: 8px; font-weight: 900; letter-spacing: .4px; padding: 1px 4px; border-radius: 4px; margin-left: 6px; vertical-align: middle; color: #0c0d12; }
  .m .who .mvp.match { background: var(--warn); box-shadow: 0 0 10px -2px var(--warn); }
  .m .who .mvp.team { background: color-mix(in srgb, var(--warn) 55%, var(--line2)); color: var(--text); }
  .m .who .mp { font-size: 11px; color: var(--muted); font-weight: 600; text-transform: uppercase; letter-spacing: .3px; margin-top: 1px; }
  .m .result { text-align: center; }
  .m .result .rr2 { font-weight: 900; font-size: 15px; letter-spacing: .5px; }
  .m.win .result .rr2 { color: var(--good); } .m.loss .result .rr2 { color: var(--bad); }
  .m .result .wl { font-size: 9.5px; font-weight: 800; letter-spacing: .6px; text-transform: uppercase; color: var(--dim); margin-top: 2px; }
  .m .kda .nums { font-size: 14px; font-weight: 700; } .m .kda .nums b { font-weight: 900; }
  .m .kda .kdr { font-size: 11px; color: var(--muted); font-weight: 700; margin-top: 1px; }
  .m .acs, .m .hs { text-align: center; }
  .m .acs .v2 { font-weight: 900; font-size: 15px; }
  .m .hs .v2 { font-weight: 800; font-size: 14px; color: var(--muted); }
  .m .acs .l2, .m .hs .l2 { font-size: 8.5px; font-weight: 800; color: var(--dim); text-transform: uppercase; letter-spacing: .6px; margin-top: 2px; }
  .acs .v2.g { color: var(--good); } .acs .v2.b { color: var(--bad); } .acs .v2.n { color: var(--text); }
</style>
