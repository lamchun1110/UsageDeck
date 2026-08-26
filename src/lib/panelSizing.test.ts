import { describe, expect, it } from 'vitest';
import {
  PANEL_MIN_HEIGHT,
  panelMaximumHeight,
  panelTargetHeight,
  screenPanelHeight,
  shouldDeferPanelFit,
} from './panelSizing';

describe('panel sizing', () => {
  it('keeps dashboard geometry fixed while provider data is refreshing', () => {
    expect(shouldDeferPanelFit('dashboard', true)).toBe(true);
    expect(shouldDeferPanelFit('dashboard', false)).toBe(false);
    expect(shouldDeferPanelFit('settings', true)).toBe(false);
  });

  it('uses the native panel floor for short content', () => {
    expect(panelTargetHeight(120, 900)).toBe(PANEL_MIN_HEIGHT);
  });

  it('caps the panel at 85% of the active monitor work area', () => {
    expect(panelMaximumHeight(700)).toBe(595);
    expect(panelTargetHeight(900, 700)).toBe(595);
  });

  it('rounds content-sized panels up between the dynamic bounds', () => {
    expect(panelTargetHeight(487.2, 1080)).toBe(488);
  });

  it('keeps Settings at the dashboard height while content screens size independently', () => {
    expect(screenPanelHeight('dashboard', 610, 540)).toBe(610);
    expect(screenPanelHeight('settings', 850, 540)).toBe(540);
    expect(screenPanelHeight('customize', 320, 540)).toBe(320);
    expect(screenPanelHeight('provider:codex', 410, 540)).toBe(410);
  });
});
