<script lang="ts">
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import { t } from './i18n.svelte';

  interface Props {
    title: string;
    message: string;
    confirmLabel: string;
    pending?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { title, message, confirmLabel, pending = false, onConfirm, onCancel }: Props = $props();
</script>

<Sheet
  role="alertdialog"
  labelledby="confirmation-title"
  describedby="confirmation-message"
  dismissible={!pending}
  onDismiss={onCancel}
>
  <span class="confirmation-sheet__icon"><Icon name="warning" size={18} strokeWidth={1.9} /></span>
  <div class="confirmation-sheet__copy">
    <h1 id="confirmation-title">{title}</h1>
    <p id="confirmation-message">{message}</p>
  </div>
  <div class="confirmation-sheet__actions">
    <button type="button" disabled={pending} onclick={onCancel}>{t('sheet.cancel')}</button>
    <button class="confirmation-sheet__confirm" type="button" disabled={pending} onclick={onConfirm}
      >{pending ? t('sheet.pending') : confirmLabel}</button
    >
  </div>
</Sheet>

<style>
  :global {
    .confirmation-sheet {
      display: grid;
      grid-template-columns: 30px 1fr;
      gap: 0 10px;
      padding: 17px;
    }

    .confirmation-sheet__icon {
      display: grid;
      width: 30px;
      height: 30px;
      border-radius: 9px;
      color: var(--meter-critical);
      background: color-mix(in srgb, var(--meter-critical) 13%, transparent);
      place-items: center;
    }

    .confirmation-sheet__copy {
      min-width: 0;
      padding-top: 1px;
    }

    .confirmation-sheet h1 {
      margin: 0 0 5px;
      font-size: 13px;
      font-weight: 650;
      letter-spacing: -0.01em;
    }

    .confirmation-sheet p {
      margin: 0;
      color: var(--secondary);
      font-size: 11px;
      line-height: 15px;
    }

    .confirmation-sheet__actions {
      display: flex;
      grid-column: 1 / -1;
      justify-content: flex-end;
      gap: 8px;
      margin-top: 16px;
    }

    .confirmation-sheet__actions button {
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
      transition:
        background-color 120ms ease,
        border-color 120ms ease,
        box-shadow 120ms ease,
        transform 90ms ease;
    }

    .confirmation-sheet__actions button:hover:not(:disabled) {
      background: color-mix(in srgb, var(--text) 11%, var(--tray));
    }

    .confirmation-sheet__actions button:active:not(:disabled) {
      transform: scale(0.97);
    }

    .confirmation-sheet__actions button:focus-visible {
      outline: 2px solid color-mix(in srgb, var(--meter-fill) 65%, transparent);
      outline-offset: 2px;
    }

    .confirmation-sheet__actions .confirmation-sheet__confirm {
      border-color: color-mix(in srgb, var(--meter-critical) 80%, #000);
      color: var(--on-fill);
      background: var(--meter-critical);
      box-shadow: 0 1px 3px color-mix(in srgb, var(--meter-critical) 34%, transparent);
    }

    .confirmation-sheet__actions .confirmation-sheet__confirm:hover:not(:disabled) {
      background: color-mix(in srgb, var(--meter-critical) 88%, #000);
    }

    .confirmation-sheet__actions button:disabled {
      cursor: default;
      opacity: 0.58;
    }
  }
</style>
