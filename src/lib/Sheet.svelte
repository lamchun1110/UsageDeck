<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';

  interface Props {
    /** Accessible name for the dialog, when there is no visible title id. */
    label?: string;
    /** Element id of the visible title, for aria-labelledby. */
    labelledby?: string;
    /** Element id of the description, for aria-describedby. */
    describedby?: string;
    role?: 'dialog' | 'alertdialog';
    /** When false, Escape cannot dismiss the sheet (e.g. while a request is pending). */
    dismissible?: boolean;
    /** Whether activating the backdrop itself dismisses the sheet. */
    dismissOnBackdrop?: boolean;
    /** Center in the viewport (about card) instead of the top-centered sheet position. */
    centered?: boolean;
    /** Solid dim backdrop without blur (about card) instead of the frosted sheet backdrop. */
    plain?: boolean;
    /** Render the backdrop without card chrome, so the caller styles its own surface. */
    chromeless?: boolean;
    /**
     * Overrides the default focus restore target (the element focused before
     * the sheet opened), for flows like an options menu whose summary must
     * regain focus regardless of what jsdom or the browser had focused.
     */
    restoreFocusTo?: () => HTMLElement | null;
    onDismiss: () => void;
    children: Snippet;
  }

  let {
    label,
    labelledby,
    describedby,
    role = 'dialog',
    dismissible = true,
    dismissOnBackdrop = false,
    centered = false,
    plain = false,
    chromeless = false,
    restoreFocusTo,
    onDismiss,
    children,
  }: Props = $props();

  let sheet = $state<HTMLElement>();

  onMount(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => {
      const target =
        sheet?.querySelector<HTMLElement>(
          'input:not(:disabled), button:not(:disabled), select:not(:disabled), textarea:not(:disabled)',
        ) ?? sheet;
      target?.focus();
    });
    return () => (restoreFocusTo?.() ?? previousFocus)?.focus();
  });

  function focusables() {
    return [
      ...(sheet?.querySelectorAll<HTMLElement>(
        'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])',
      ) ?? []),
    ];
  }

  function handleKeydown(event: KeyboardEvent) {
    // Keep keystrokes from reaching the panel beneath the modal surface.
    event.stopPropagation();
    if (event.key === 'Escape') {
      event.preventDefault();
      if (dismissible) onDismiss();
      return;
    }
    if (event.key !== 'Tab' || !sheet) return;
    const controls = focusables();
    if (controls.length === 0) {
      event.preventDefault();
      sheet.focus();
      return;
    }
    const first = controls[0];
    const last = controls.at(-1)!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function handleBackdropPointerDown(event: PointerEvent) {
    // The panel beneath must not see pointer events aimed at the modal.
    event.stopPropagation();
  }

  function handleBackdropClick(event: MouseEvent) {
    event.stopPropagation();
    if (dismissible && dismissOnBackdrop && event.target === event.currentTarget) {
      onDismiss();
    }
  }
</script>

<div
  class="sheet-backdrop"
  class:sheet-backdrop--centered={centered}
  class:sheet-backdrop--plain={plain}
  role="presentation"
  data-testid="sheet-backdrop"
  onpointerdown={handleBackdropPointerDown}
  onclick={handleBackdropClick}
  onkeydown={handleKeydown}
>
  <div
    bind:this={sheet}
    class="sheet-surface"
    class:sheet-surface--chromeless={chromeless}
    {role}
    tabindex="-1"
    aria-modal="true"
    aria-label={label}
    aria-labelledby={labelledby}
    aria-describedby={describedby}
  >
    {@render children()}
  </div>
</div>

<style>
  :global {
    .sheet-backdrop {
      position: absolute;
      z-index: 120;
      display: grid;
      padding: 48px 18px 18px;
      background: color-mix(in srgb, #000 24%, transparent);
      backdrop-filter: blur(7px) saturate(0.9);
      animation: sheet-backdrop-in var(--motion-switch) both;
      inset: 0;
      place-items: start center;
    }

    .sheet-backdrop--centered {
      padding: 18px;
      place-items: center;
    }

    .sheet-backdrop--plain {
      background: rgba(0, 0, 0, 0.28);
      backdrop-filter: blur(6px);
    }

    .sheet-surface {
      width: min(252px, 100%);
      box-sizing: border-box;
      border: 1px solid color-mix(in srgb, var(--text) 11%, transparent);
      border-radius: 15px;
      color: var(--text);
      background: color-mix(in srgb, var(--tray) 96%, transparent);
      box-shadow:
        0 22px 60px rgba(0, 0, 0, 0.3),
        0 2px 8px rgba(0, 0, 0, 0.14);
      backdrop-filter: blur(24px) saturate(1.18);
      animation: sheet-surface-in var(--motion-spring) both;
    }

    .sheet-surface:focus {
      outline: none;
    }

    .sheet-surface--chromeless {
      width: auto;
      border: 0;
      background: none;
      box-shadow: none;
      backdrop-filter: none;
      animation: none;
    }

    @keyframes sheet-backdrop-in {
      from {
        background-color: transparent;
        backdrop-filter: blur(0) saturate(1);
      }
    }

    @keyframes sheet-surface-in {
      from {
        opacity: 0;
        transform: translateY(-12px) scale(0.975);
      }
    }

    :global(:root[data-reduced-motion]) .sheet-backdrop,
    :global(:root[data-reduced-motion]) .sheet-surface {
      animation-duration: 0ms;
    }
  }
</style>
