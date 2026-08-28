import { describe, expect, it, vi } from 'vitest';
import { AppKeyboardController } from './appKeyboardController';
import type { AppScreen } from './windowController';

function setup(activeScreen: AppScreen = 'dashboard') {
  let screen = activeScreen;
  let aboutOpen = false;
  const actions = {
    closeAbout: vi.fn(() => (aboutOpen = false)),
    back: vi.fn(),
    openCustomize: vi.fn(() => (screen = 'customize')),
    toggleSettings: vi.fn(),
    refresh: vi.fn(),
    undoCustomization: vi.fn(),
    quit: vi.fn(),
  };
  const controller = new AppKeyboardController({
    screen: () => screen,
    aboutOpen: () => aboutOpen,
    ...actions,
  });
  return { controller, actions, openAbout: () => (aboutOpen = true) };
}

describe('AppKeyboardController', () => {
  it('routes Escape to the topmost app-owned surface', () => {
    const { controller, actions, openAbout } = setup();
    openAbout();

    controller.handleKeydown(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(actions.closeAbout).toHaveBeenCalledOnce();
    expect(actions.back).not.toHaveBeenCalled();

    controller.handleKeydown(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(actions.back).toHaveBeenCalledOnce();
  });

  it('does not steal dashboard Enter from interactive controls', () => {
    const { controller, actions } = setup();
    const button = document.createElement('button');
    const owned = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true });
    button.addEventListener('keydown', controller.handleKeydown);
    button.dispatchEvent(owned);
    expect(actions.openCustomize).not.toHaveBeenCalled();

    controller.handleKeydown(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(actions.openCustomize).toHaveBeenCalledOnce();
  });

  it('preserves editing shortcuts while routing app shortcuts', () => {
    const { controller, actions } = setup();
    const input = document.createElement('input');
    input.addEventListener('keydown', controller.handleKeydown);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true }));
    expect(actions.undoCustomization).not.toHaveBeenCalled();

    controller.handleKeydown(new KeyboardEvent('keydown', { key: 'r', ctrlKey: true }));
    controller.handleKeydown(new KeyboardEvent('keydown', { key: 'q', ctrlKey: true }));
    controller.handleKeydown(new KeyboardEvent('keydown', { key: ',', ctrlKey: true }));
    expect(actions.refresh).toHaveBeenCalledOnce();
    expect(actions.quit).toHaveBeenCalledOnce();
    expect(actions.toggleSettings).toHaveBeenCalledOnce();
  });
});
