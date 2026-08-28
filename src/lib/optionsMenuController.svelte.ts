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
    if (!menu || event.key !== 'Escape' || !menu.open) return;
    event.preventDefault();
    event.stopPropagation();
    this.close(true);
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
