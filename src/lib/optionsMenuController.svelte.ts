export class OptionsMenuController {
  optionsElement = $state<HTMLDetailsElement>();
  shareElement = $state<HTMLDetailsElement>();
  shareOpen = $state(false);

  acceptShareToggle(open: boolean) {
    this.shareOpen = open;
  }

  handleKey(event: KeyboardEvent) {
    const menu = (event.currentTarget as HTMLElement).closest<HTMLDetailsElement>(
      'details.options-menu',
    );
    if (!menu || !menu.open) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close(true);
      return;
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    const items = [...menu.querySelectorAll<HTMLElement>('.menu-item:not(:disabled)')];
    if (items.length === 0) return;
    event.preventDefault();
    const current = items.indexOf(document.activeElement as HTMLElement);
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? items.length - 1
          : event.key === 'ArrowDown'
            ? current < 0
              ? 0
              : (current + 1) % items.length
            : current < 0
              ? items.length - 1
              : (current - 1 + items.length) % items.length;
    items[next].focus();
  }

  handleWindowPointerDown(event: PointerEvent) {
    if (
      this.optionsElement?.open &&
      event.target instanceof Node &&
      !this.optionsElement.contains(event.target)
    ) {
      this.close();
    }
  }

  close(restoreFocus = false) {
    if (this.shareElement?.open) this.shareElement.open = false;
    this.shareOpen = false;
    if (!this.optionsElement?.open) return;
    this.optionsElement.open = false;
    if (restoreFocus) this.restoreTarget()?.focus();
  }

  restoreTarget() {
    return this.optionsElement?.querySelector<HTMLElement>(':scope > summary') ?? null;
  }
}
