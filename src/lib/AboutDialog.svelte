<script lang="ts">
  import { t } from './i18n.svelte';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import UsageDeckMark from './UsageDeckMark.svelte';

  interface Props {
    version: string;
    restoreFocusTo: () => HTMLElement | null;
    onDismiss: () => void;
  }

  let { version, restoreFocusTo, onDismiss }: Props = $props();
</script>

<Sheet
  label={t('app.menu.about')}
  centered
  plain
  chromeless
  dismissOnBackdrop
  {restoreFocusTo}
  {onDismiss}
>
  <div class="about-card">
    <button
      class="about-card__close"
      type="button"
      aria-label={t('app.closeAbout')}
      onclick={onDismiss}><Icon name="close" size={11} strokeWidth={2.3} /></button
    >
    <UsageDeckMark size={44} />
    <h1>UsageDeck</h1>
    <p>{t('app.version', { version })}</p>
    <small>{t('app.aboutTagline')}</small>
  </div>
</Sheet>

<style>
  .about-card {
    position: relative;
    display: flex;
    width: 230px;
    align-items: center;
    padding: 24px 20px 20px;
    border: 1px solid var(--separator);
    border-radius: 16px;
    color: var(--text);
    background: var(--tray);
    box-shadow: 0 18px 55px rgba(0, 0, 0, 0.35);
    flex-direction: column;
    animation: detail-in var(--motion-spring) both;
  }

  .about-card h1 {
    margin: 10px 0 2px;
    font-size: 17px;
  }

  .about-card p,
  .about-card small {
    margin: 0;
    color: var(--secondary);
    font-size: 10px;
    text-align: center;
  }

  .about-card__close {
    position: absolute;
    top: 8px;
    right: 8px;
    display: grid;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    color: var(--secondary);
    background: var(--button-hover);
    cursor: pointer;
    place-items: center;
    transition:
      color var(--motion-switch),
      background var(--motion-switch),
      transform var(--motion-switch);
  }

  .about-card__close:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 14%, transparent);
  }

  .about-card__close:active {
    transform: scale(0.92);
  }

  .about-card__close:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--meter-fill) 55%, transparent);
    outline-offset: 1px;
  }
</style>
