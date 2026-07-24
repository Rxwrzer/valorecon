<svelte:options runes={true} />
<script lang="ts">
  let { width = "100%", height = "16px", radius = "6px", style = "" } = $props<{
    width?: string;
    height?: string;
    radius?: string;
    style?: string;
  }>();
</script>

<div class="skel" style="width:{width};height:{height};border-radius:{radius};{style}"></div>

<style>
.skel {
  background: linear-gradient(
    90deg,
    var(--panel3) 25%,
    color-mix(in srgb, var(--line2) 60%, var(--panel3)) 50%,
    var(--panel3) 75%
  );
  background-size: 200% 100%;
  animation: shimmer 1.4s ease-in-out infinite;
  display: inline-block;
}
@keyframes shimmer {
  0%   { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}
</style>
