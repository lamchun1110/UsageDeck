import type { AppScreen } from './windowController';

interface AppKeyboardControllerOptions {
  screen: () => AppScreen;
  aboutOpen: () => boolean;
  closeAbout: () => void;
  back: () => void;
  openCustomize: () => void;
  toggleSettings: () => void;
  refresh: () => void;
  undoCustomization: () => void;
  quit: () => void;
}

function ownsEnterKey(target: EventTarget | null) {
  if (!(target instanceof Element)) return false;
  return (
    target.closest(
      'button, a, input, select, textarea, summary, [contenteditable], [role="button"], [role="menuitem"], [role="option"], [role="combobox"]',
    ) !== null
  );
}

function ownsEditingShortcut(target: EventTarget | null) {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
}

export class AppKeyboardController {
  constructor(private readonly options: AppKeyboardControllerOptions) {}

  handleKeydown = (event: KeyboardEvent) => {
    if (event.defaultPrevented || event.isComposing) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      if (this.options.aboutOpen()) this.options.closeAbout();
      else this.options.back();
    } else if (
      event.key === 'Enter' &&
      this.options.screen() === 'dashboard' &&
      !ownsEnterKey(event.target)
    ) {
      event.preventDefault();
      this.options.openCustomize();
    } else if ((event.ctrlKey || event.metaKey) && event.key === ',') {
      event.preventDefault();
      this.options.toggleSettings();
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'r') {
      event.preventDefault();
      this.options.refresh();
    } else if (
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === 'z' &&
      !ownsEditingShortcut(event.target)
    ) {
      event.preventDefault();
      this.options.undoCustomization();
    } else if (
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === 'q' &&
      !ownsEditingShortcut(event.target)
    ) {
      event.preventDefault();
      this.options.quit();
    }
  };

  listen(target: Document = document) {
    target.addEventListener('keydown', this.handleKeydown);
    return () => target.removeEventListener('keydown', this.handleKeydown);
  }
}
