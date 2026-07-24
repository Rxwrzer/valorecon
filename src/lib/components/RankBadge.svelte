<svelte:options runes={true} />
<script lang="ts">
  import type { RankInfo } from "$lib/api";
  import Skeleton from "./Skeleton.svelte";

  let { rank, pending = false, size = "normal" } = $props<{
    rank: RankInfo;
    pending?: boolean;
    size?: "normal" | "small";
  }>();
</script>

{#if pending}
  <div class="rank rank-{size}">
    <Skeleton width="32px" height="32px" radius="8px" />
    <div>
      <Skeleton width="64px" height="12px" radius="4px" />
      <Skeleton width="40px" height="10px" radius="4px" style="margin-top:4px" />
    </div>
  </div>
{:else}
  <div class="rank rank-{size}" style="--tier:{rank.tier_color}">
    {#if rank.tier_icon}
      <img src={rank.tier_icon} alt={rank.tier_name}
           style="width:{size==='small'?'22px':'32px'};height:{size==='small'?'22px':'32px'}"
           onerror={(e) => (e.currentTarget as HTMLElement).style.display='none'} />
    {/if}
    <div>
      <div class="t" style="color:{rank.tier_color}">{rank.tier_name}</div>
      {#if size === "normal" && rank.tier > 0}
        <div class="rr">{rank.rr} RR</div>
      {/if}
    </div>
  </div>
{/if}

<style>
.rank {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 3px 9px 3px 5px;
  border-radius: 9px;
  background: color-mix(in srgb, var(--tier, #5a5a5a) 13%, transparent);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tier, #5a5a5a) 32%, transparent);
}
.rank-small {
  gap: 7px;
  padding: 2px 8px 2px 4px;
  background: transparent;
  box-shadow: none;
}
.t {
  font-weight: 900;
  font-size: 12px;
  line-height: 1.1;
  text-transform: uppercase;
  letter-spacing: .3px;
  text-shadow: 0 0 10px color-mix(in srgb, var(--tier, #5a5a5a) 45%, transparent);
}
.rr { color: var(--muted); font-size: 10px; font-weight: 800; letter-spacing: .3px; }
</style>
