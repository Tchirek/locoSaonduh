<script lang="ts">
  let ready = $state(false);

  $effect(() => {
    ready = true;
  });
</script>

<main>
  <p class="eyebrow">audio, in the room</p>
  <h1>locoSaonduh</h1>
  <p class="pending">{ready ? "core is being tuned." : "opening the room."}</p>
</main>

<style>
  main {
    min-height: 100vh;
    display: grid;
    place-content: center;
    gap: 12px;
    text-align: center;
  }

  .eyebrow {
    margin: 0;
    color: var(--ink-muted);
    font-family: var(--mono);
    font-size: 12px;
    letter-spacing: 0.08em;
  }

  h1 {
    margin: 0;
    font-size: clamp(40px, 7vw, 80px);
    letter-spacing: -0.055em;
  }

  .pending {
    margin: 0;
    color: var(--ink-muted);
  }
</style>
