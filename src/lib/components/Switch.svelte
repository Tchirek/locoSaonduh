<script lang="ts">
  let {
    checked = false,
    disabled = false,
    busy = false,
    label,
    onchange,
  }: {
    checked?: boolean;
    disabled?: boolean;
    busy?: boolean;
    label: string;
    onchange: (next: boolean) => void;
  } = $props();
</script>

<button
  type="button"
  class="switch"
  class:on={checked}
  class:busy
  role="switch"
  aria-checked={checked}
  aria-label={label}
  aria-busy={busy}
  disabled={disabled || busy}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    width: 40px;
    height: 24px;
    flex: none;
    border-radius: 12px;
    background: var(--tile);
    transition: background-color 160ms ease;
  }

  .switch.on {
    background: var(--accent);
  }

  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--knob);
    box-shadow: 0 1px 2px rgb(0 0 0 / 18%);
    transition: translate 160ms ease;
  }

  .switch.on .knob {
    translate: 16px 0;
  }

  .switch.busy {
    opacity: 1;
    cursor: progress;
  }

  .switch.busy .knob {
    animation: nudge 900ms ease-in-out infinite;
  }

  @keyframes nudge {
    50% {
      translate: 8px 0;
    }
  }
</style>
