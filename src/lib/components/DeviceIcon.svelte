<script lang="ts">
  import { Laptop, Monitor, Smartphone, SquareTerminal } from "@lucide/svelte";

  let { platform = "" }: { platform?: string } = $props();

  let Icon = $derived(
    platform === "android" || platform === "ios"
      ? Smartphone
      : platform === "macos" || platform === "windows"
        ? Monitor
        : platform === "linux"
          ? SquareTerminal
          : Laptop,
  );
</script>

<span class="device-icon" aria-hidden="true">
  <Icon size={20} strokeWidth={1.6} />
</span>

<style>
  .device-icon {
    display: inline-grid;
    flex: none;
    place-items: center;
    color: var(--muted);
  }
</style>
