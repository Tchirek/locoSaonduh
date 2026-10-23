<script lang="ts">
  import { Laptop, Mic, Monitor, Smartphone, SquareTerminal, Volume2 } from "@lucide/svelte";

  let {
    platform = "",
    size = 20,
    active = false,
    solid = false,
  }: { platform?: string; size?: number; active?: boolean; solid?: boolean } = $props();

  let Icon = $derived(
    platform === "android" || platform === "ios"
      ? Smartphone
      : platform === "macos" || platform === "windows"
        ? Monitor
        : platform === "linux"
          ? SquareTerminal
          : platform === "system"
            ? Volume2
            : platform === "microphone"
              ? Mic
              : Laptop,
  );
</script>

<span class="device-icon" class:active class:solid aria-hidden="true">
  <Icon {size} strokeWidth={1.6} />
</span>

<style>
  .device-icon {
    display: inline-grid;
    width: 40px;
    height: 40px;
    flex: none;
    place-items: center;
    border-radius: 10px;
    background: var(--tile);
    color: var(--ink-2);
    transition:
      background-color 140ms ease,
      color 140ms ease;
  }

  .device-icon.active {
    color: var(--on-accent-soft);
    background: var(--accent-soft);
  }

  .device-icon.solid {
    color: var(--on-accent);
    background: var(--accent);
  }
</style>
