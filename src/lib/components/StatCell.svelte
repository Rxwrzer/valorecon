<svelte:options runes={true} />
<script lang="ts">
  import type { PlayerStats } from "$lib/api";
  import Skeleton from "./Skeleton.svelte";

  let { stats, pending = false } = $props<{
    stats: PlayerStats | null;
    pending?: boolean;
  }>();

  function cls(v: number | null, good: number, bad: number) {
    if (v == null) return "muted";
    return v >= good ? "g" : v < bad ? "b" : "n";
  }
</script>

{#if pending}
  {#each ["KDA","ACS","ADR","HS%","WR"] as lbl}
    <div class="stat">
      <Skeleton width="36px" height="15px" radius="4px" />
      <div class="sl">{lbl}</div>
    </div>
  {/each}
{:else if !stats}
  {#each ["KDA","ACS","ADR","HS%","WR"] as lbl}
    <div class="stat">
      <div class="sv muted">—</div>
      <div class="sl">{lbl}</div>
    </div>
  {/each}
{:else}
  <div class="stat">
    <div class="sv {cls(stats.kda, 1.3, 1.0)}">{stats.kda?.toFixed(2) ?? "—"}</div>
    <div class="ksub">{stats.avg_k}/{stats.avg_d}/{stats.avg_a}</div>
    <div class="sl">KDA</div>
  </div>
  <div class="stat">
    <div class="sv {cls(stats.acs, 250, 150)}">{stats.acs ?? "—"}</div>
    <div class="sl">ACS</div>
  </div>
  <div class="stat">
    <div class="sv {cls(stats.adr, 140, 100)}">{stats.adr ?? "—"}</div>
    <div class="sl">ADR</div>
  </div>
  <div class="stat">
    <div class="sv {cls(stats.hs, 25, 0)}">{stats.hs != null ? stats.hs + "%" : "—"}</div>
    <div class="sl">HS%</div>
  </div>
  <div class="stat">
    <div class="sv {cls(stats.winrate, 55, 45)}">{stats.winrate != null ? stats.winrate + "%" : "—"}</div>
    <div class="sl">WR·{stats.games}</div>
  </div>
{/if}

<style>
.stat { text-align: center; line-height: 1.12; }
.sv { font-weight: 900; font-size: 15px; letter-spacing: .2px; }
.sl { font-size: 8.5px; color: var(--dim); text-transform: uppercase; letter-spacing: .7px; margin-top: 2px; font-weight: 800; }
.ksub { font-size: 9.5px; color: var(--muted); font-weight: 700; margin-top: 1px; }
</style>
