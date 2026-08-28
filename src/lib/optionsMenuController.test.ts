import { afterEach, describe, expect, it, vi } from 'vitest';
import { OptionsMenuController } from './optionsMenuController.svelte';

afterEach(() => document.body.replaceChildren());

function menuFixture() {
  const options = document.createElement('details');
  options.className = 'options-menu';
  options.innerHTML = '<summary>Options</summary><div><details class="share-menu"></details></div>';
  const share = options.querySelector<HTMLDetailsElement>('.share-menu')!;
  document.body.append(options);
  const controller = new OptionsMenuController();
  controller.optionsElement = options;
  controller.shareElement = share;
  return { controller, options, share, summary: options.querySelector('summary')! };
}

describe('OptionsMenuController', () => {
  it('closes both menu levels and restores summary focus on Escape', () => {
    const { controller, options, share, summary } = menuFixture();
    options.open = true;
    share.open = true;
    controller.acceptShareToggle(true);
    const event = {
      currentTarget: share,
      key: 'Escape',
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    } as unknown as KeyboardEvent;

    controller.handleKey(event);

    expect(options.open).toBe(false);
    expect(share.open).toBe(false);
    expect(controller.shareOpen).toBe(false);
    expect(summary).toHaveFocus();
  });

  it('closes an open menu for a pointer press outside it', () => {
    const { controller, options } = menuFixture();
    options.open = true;

    controller.handleWindowPointerDown({ target: document.body } as unknown as PointerEvent);

    expect(options.open).toBe(false);
  });
});
