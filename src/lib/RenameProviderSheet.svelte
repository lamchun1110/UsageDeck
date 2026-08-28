<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Sheet from './Sheet.svelte';
  import { t } from './i18n.svelte';

  interface Props {
    initialValue: string;
    onRename: (name: string) => void;
    onCancel: () => void;
  }

  let { initialValue, onRename, onCancel }: Props = $props();
  let draft = $state('');
  let input = $state<HTMLInputElement>();

  onMount(() => {
    draft = initialValue;
    // Sheet focuses the input as the first control; preselect the current
    // name so typing replaces it in one keystroke.
    void tick().then(() => input?.select());
  });
</script>

<Sheet labelledby="rename-title" describedby="rename-message" onDismiss={onCancel}>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      onRename(draft);
    }}
  >
    <h1 id="rename-title">{t('rename.title')}</h1>
    <input
      bind:this={input}
      bind:value={draft}
      type="text"
      maxlength="48"
      placeholder={t('rename.namePlaceholder')}
      aria-label={t('rename.nameAria')}
    />
    <p id="rename-message">{t('rename.hint')}</p>
    <div class="rename-sheet__actions">
      <button type="button" onclick={onCancel}>{t('rename.cancel')}</button>
      <button class="rename-sheet__confirm" type="submit">{t('rename.confirm')}</button>
    </div>
  </form>
</Sheet>

<style>
  :global {
    form {
      display: grid;
      gap: 10px;
      padding: 17px;
    }

    form h1 {
      margin: 0;
      font-size: 13px;
      font-weight: 650;
      letter-spacing: -0.01em;
    }

    form input {
      width: 100%;
      min-width: 0;
      box-sizing: border-box;
      padding: 7px 9px;
      border: 1px solid var(--separator);
      border-radius: 8px;
      outline: none;
      color: var(--text);
      background: var(--card);
      font: inherit;
      font-size: 12px;
    }

    form input:focus {
      border-color: var(--meter-fill);
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--meter-fill) 20%, transparent);
    }

    form p {
      margin: -2px 0 0;
      color: var(--secondary);
      font-size: 11px;
      line-height: 15px;
    }

    .rename-sheet__actions {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
      margin-top: 6px;
    }

    .rename-sheet__actions button {
      min-width: 76px;
      min-height: 31px;
      padding: 6px 12px;
      border: 1px solid color-mix(in srgb, var(--text) 9%, transparent);
      border-radius: 8px;
      color: var(--text);
      background: color-mix(in srgb, var(--text) 7%, var(--tray));
      box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
      font-size: 11px;
      font-weight: 600;
      cursor: pointer;
    }

    .rename-sheet__actions button:hover {
      background: color-mix(in srgb, var(--text) 11%, var(--tray));
    }

    .rename-sheet__actions button:active {
      transform: scale(0.97);
    }

    .rename-sheet__actions button:focus-visible {
      outline: 2px solid color-mix(in srgb, var(--meter-fill) 65%, transparent);
      outline-offset: 2px;
    }

    .rename-sheet__actions .rename-sheet__confirm {
      border-color: color-mix(in srgb, var(--meter-fill) 80%, #000);
      color: var(--on-fill);
      background: var(--meter-fill);
      box-shadow: 0 1px 3px color-mix(in srgb, var(--meter-fill) 34%, transparent);
    }

    .rename-sheet__actions .rename-sheet__confirm:hover {
      background: color-mix(in srgb, var(--meter-fill) 88%, #000);
    }
  }
</style>
