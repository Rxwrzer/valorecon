<svelte:options runes={true} />
<script lang="ts">
  let msg = $state("");
  let visible = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  export function show(text: string) {
    msg = text;
    visible = true;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { visible = false; }, 1400);
  }
</script>

<div class="toast" class:show={visible}>{msg}</div>

<style>
.toast {
  position: fixed;
  bottom: 18px;
  left: 50%;
  transform: translateX(-50%) translateY(10px);
  background: var(--panel2);
  border: 1px solid var(--good);
  color: var(--good);
  font-weight: 800;
  font-size: 12.5px;
  padding: 8px 15px;
  border-radius: 9px;
  box-shadow: 0 10px 26px -10px #000;
  opacity: 0;
  pointer-events: none;
  transition: opacity .18s, transform .18s;
  z-index: 60;
  white-space: nowrap;
}
.toast.show { opacity: 1; transform: translateX(-50%) translateY(0); }
</style>
