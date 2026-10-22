<script lang="ts">
  import { Laptop, Monitor, Smartphone, SquareTerminal } from "@lucide/svelte";

  let { platform = "", size = 19 }: { platform?: string; size?: number } =
    $props();

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
  <Icon size={size} strokeWidth={1.55} />
</span>

<style>
  .device-icon {
    display: inline-grid;
    width: 34px;
    height: 34px;
    flex: none;
    place-items: center;
    color: var(--ink-secondary);
  }
</style>
