<svelte:options runes={true} />
<script lang="ts">
  const PARTY_COLORS = ['#5b8cff','#ff9f43','#a66bff','#2dd4bf','#f472b6','#a3e635'];

  let { group, confirmed } = $props<{ group: number; confirmed: boolean }>();

  const color = $derived(PARTY_COLORS[(group - 1) % PARTY_COLORS.length]);
  const label = $derived((confirmed ? "" : "~") + "P" + group);
  const title = $derived(confirmed
    ? "In your party"
    : "Likely premade — inferred from recent games");
</script>

<span
  class="chip {confirmed ? '' : 'inferred'}"
  style="--pc:{color}"
  {title}
>
  {label}
</span>

<style>
.chip {
  display: inline-block;
  font-size: 8.5px;
  font-weight: 900;
  letter-spacing: .4px;
  padding: 1px 6px;
  border-radius: 5px;
  margin-left: 7px;
  vertical-align: middle;
  background: var(--pc);
  color: #0b0d12;
  text-transform: uppercase;
}
.chip.inferred {
  background: transparent;
  color: var(--pc);
  box-shadow: inset 0 0 0 1px var(--pc);
}
</style>
