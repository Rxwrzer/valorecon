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

  // Aggregate stats over the returned matches.
  const summary = $derived.by(() => {
    const gs = result?.matches;
    if (!gs?.length) return null;
    let k = 0, d = 0, a = 0, score = 0, rounds = 0, hsW = 0, wW = 0, wins = 0, decided = 0;
    const agentCount: Record<string, number> = {};
    for (const g of gs) {
      k += g.kills; d += g.deaths; a += g.assists;
      score += g.score || 0; rounds += g.rounds || 0;
      // hs is a per-game %, weight by that game's rounds for a fair mean.
      const w = g.rounds || 1;
      hsW += (g.hs || 0) * w; wW += w;
      if (g.won === true) { wins++; decided++; }
      else if (g.won === false) { decided++; }
      if (g.agent) agentCount[g.agent] = (agentCount[g.agent] ?? 0) + 1;
    }
    const n = gs.length;
    const topAgent = Object.entries(agentCount).sort((x, y) => y[1] - x[1])[0]?.[0] ?? "—";
    return {
      games: n,
      kda: d > 0 ? ((k + a) / d).toFixed(2) : (k + a).toFixed(2),
      avgK: (k / n).toFixed(1),
      avgD: (d / n).toFixed(1),
      avgA: (a / n).toFixed(1),
      acs: rounds > 0 ? Math.round(score / rounds) : null,
      hs: wW > 0 ? Math.round(hsW / wW) : Math.round(gs.reduce((s, g) => s + g.hs, 0) / n),
      wr: decided > 0 ? Math.round((wins / decided) * 100) : null,
      topAgent,
    };
  });
</script>

<div class="mhead"><h2>Look Up a Player</h2></div>

<div class="searchbar">
  <input
    bind:value={riotId}
    placeholder="Name#TAG"
    autocomplete="off"
    onkeydown={(e) => { if (e.key === "Enter") doLookup(); }}
  />
  <button class="primary" onclick={doLookup} disabled={loading}>
    {loading ? "Searching…" : "Search"}
  </button>
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
  <div class="cards">
    <div class="card">
      <div class="k">{a.name}#{a.tag}</div>
      <div class="v" style="font-size:15px">{a.region.toUpperCase()} · Lv {a.level}</div>
    </div>
    <div class="card">
      <div class="k">Current</div>
      <div class="v" style="font-size:16px">{m.current_tier_name} <small>{m.current_rr} RR</small></div>
    </div>
    <div class="card">
      <div class="k">Peak</div>
      <div class="v" style="font-size:16px">{m.peak_tier_name || "—"}</div>
    </div>
  </div>
  {#if summary}
    <div class="summary">
      <div class="stitle">Last {summary.games} comp games</div>
      <div class="sgrid">
        <div class="sstat"><div class="sv">{summary.kda}</div><div class="sl">KDA</div></div>
        <div class="sstat"><div class="sv">{summary.avgK}/{summary.avgD}/{summary.avgA}</div><div class="sl">Avg K/D/A</div></div>
        <div class="sstat"><div class="sv">{summary.acs != null ? summary.acs : "—"}</div><div class="sl">ACS</div></div>
        <div class="sstat"><div class="sv">{summary.hs}%</div><div class="sl">HS%</div></div>
        <div class="sstat"><div class="sv">{summary.wr != null ? summary.wr + "%" : "—"}</div><div class="sl">Win rate</div></div>
        <div class="sstat"><div class="sv">{summary.topAgent}</div><div class="sl">Most played</div></div>
      </div>
    </div>
  {/if}
  <div class="glist">
    {#if !result.matches?.length}
      <div class="hint muted">No recent competitive matches.</div>
    {:else}
      {#each result.matches! as g}
        <div class="grow">
          <div>{g.agent} <span class="muted">· {g.map}</span></div>
          <div class="muted">{g.kills}/{g.deaths}/{g.assists} · {g.kd} KD</div>
          <div class="muted">{g.hs}% HS</div>
          <div class="delta">
            {#if g.won === true}<span class="up">WON</span>
            {:else if g.won === false}<span class="down">LOST</span>
            {:else}<span class="muted">—</span>{/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>
{/if}

<style>
.mhead { display: flex; align-items: center; gap: 12px; margin: 0 0 16px; }
.mhead h2 { font-size: 17px; margin: 0; font-weight: 900; letter-spacing: .4px; text-transform: uppercase; border-left: 3px solid var(--accent); padding-left: 10px; }
.searchbar { display: flex; gap: 9px; margin-bottom: 16px; max-width: 440px; }
.cards { display: flex; gap: 13px; flex-wrap: wrap; margin-bottom: 16px; }
.card { position: relative; background: linear-gradient(180deg,var(--panel2),var(--panel)); border: 1px solid var(--line); border-radius: 12px; padding: 16px 18px; min-width: 152px; overflow: hidden; }
.card::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: var(--accent); }
.card .k { color: var(--muted); font-size: 11px; margin-bottom: 9px; font-weight: 800; text-transform: uppercase; letter-spacing: .6px; }
.card .v { display: flex; align-items: center; gap: 11px; font-size: 22px; font-weight: 900; }
.card small { color: var(--muted); font-weight: 700; font-size: 12px; }
.summary { background: linear-gradient(180deg,var(--panel2),var(--panel)); border: 1px solid var(--line); border-radius: 12px; padding: 14px 16px; margin-bottom: 16px; }
.stitle { color: var(--muted); font-size: 11px; font-weight: 800; text-transform: uppercase; letter-spacing: .6px; margin-bottom: 11px; }
.sgrid { display: grid; grid-template-columns: repeat(6,1fr); gap: 10px; }
.sstat { background: var(--panel2); border: 1px solid var(--line); border-radius: 9px; padding: 10px 6px; text-align: center; }
.sv { font-weight: 900; font-size: 16px; }
.sl { font-size: 8.5px; color: var(--dim); text-transform: uppercase; letter-spacing: .6px; margin-top: 3px; font-weight: 800; }
.glist { display: flex; flex-direction: column; gap: 6px; }
.grow { display: grid; grid-template-columns: 1fr 130px 62px 62px; gap: 10px; align-items: center; background: var(--panel); border: 1px solid var(--line); border-left: 3px solid var(--line2); border-radius: 8px; padding: 9px 13px; font-size: 13px; }
.delta { font-weight: 900; text-align: right; }
</style>
