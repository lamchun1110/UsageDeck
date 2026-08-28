import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  beginPanelResize: vi.fn(),
  currentPanelWidth: vi.fn(),
  getPanelHeightMode: vi.fn(),
  getPanelResizeEdge: vi.fn(),
  lockPanelResizeAxis: vi.fn(),
  setPanelHeightAutomatic: vi.fn(),
  setPanelHeightManual: vi.fn(),
  setPanelWidth: vi.fn(),
  startResizeDragging: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ startResizeDragging: mocks.startResizeDragging }),
}));

vi.mock('./backend', () => ({
  beginPanelResize: mocks.beginPanelResize,
  currentPanelWidth: mocks.currentPanelWidth,
  getPanelHeightMode: mocks.getPanelHeightMode,
  getPanelResizeEdge: mocks.getPanelResizeEdge,
  lockPanelResizeAxis: mocks.lockPanelResizeAxis,
  setPanelHeightAutomatic: mocks.setPanelHeightAutomatic,
  setPanelHeightManual: mocks.setPanelHeightManual,
  setPanelWidth: mocks.setPanelWidth,
}));

import { PANEL_MAX_WIDTH, PanelResizeController } from './panelResizeController.svelte';

describe('PanelResizeController', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    mocks.beginPanelResize.mockResolvedValue('bottom');
    mocks.currentPanelWidth.mockResolvedValue(380);
    mocks.getPanelHeightMode.mockResolvedValue('manual');
    mocks.getPanelResizeEdge.mockResolvedValue('bottom');
    mocks.lockPanelResizeAxis.mockResolvedValue(undefined);
    mocks.setPanelHeightAutomatic.mockResolvedValue(undefined);
    mocks.setPanelHeightManual.mockResolvedValue(undefined);
    mocks.setPanelWidth.mockResolvedValue(undefined);
    mocks.startResizeDragging.mockResolvedValue(undefined);
  });

  it('refreshes the native edge and height mode as one controller operation', async () => {
    const scheduleFit = vi.fn();
    const controller = new PanelResizeController({
      platform: 'windows',
      scheduleFit,
      onError: vi.fn(),
    });
    expect(controller.edge).toBe('top');

    controller.refresh();

    await vi.waitFor(() => expect(controller.edge).toBe('bottom'));
    expect(controller.heightMode).toBe('manual');
    expect(scheduleFit).not.toHaveBeenCalled();

    mocks.getPanelHeightMode.mockResolvedValue('automatic');
    controller.refreshHeightMode();
    await vi.waitFor(() => expect(controller.heightMode).toBe('automatic'));
    expect(scheduleFit).toHaveBeenCalledOnce();
  });

  it('owns the native height drag lifecycle and mirrors manual mode immediately', async () => {
    const controller = new PanelResizeController({
      platform: 'macos',
      scheduleFit: vi.fn(),
      onError: vi.fn(),
    });
    const event = {
      button: 0,
      detail: 1,
      timeStamp: 100,
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    } as unknown as PointerEvent;

    controller.handleHeightPointerDown(event);

    await vi.waitFor(() => expect(mocks.startResizeDragging).toHaveBeenCalledWith('South'));
    expect(controller.heightMode).toBe('manual');
    await vi.waitFor(() => expect(mocks.lockPanelResizeAxis).toHaveBeenCalledOnce());
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
  });

  it('clamps keyboard width changes and reports native failures', async () => {
    const onError = vi.fn();
    const controller = new PanelResizeController({
      platform: 'linux',
      scheduleFit: vi.fn(),
      onError,
    });
    mocks.currentPanelWidth.mockResolvedValue(PANEL_MAX_WIDTH - 4);
    const event = new KeyboardEvent('keydown', { key: 'ArrowRight' });

    await controller.handleWidthKeydown(event);

    expect(controller.width).toBe(PANEL_MAX_WIDTH);
    expect(mocks.setPanelWidth).toHaveBeenCalledWith(PANEL_MAX_WIDTH);
    expect(mocks.lockPanelResizeAxis).toHaveBeenCalledOnce();

    mocks.setPanelWidth.mockRejectedValueOnce(new Error('native resize unavailable'));
    await controller.handleWidthKeydown(new KeyboardEvent('keydown', { key: 'Home' }));
    expect(onError).toHaveBeenCalledWith('UsageDeck panel width could not be resized.');
  });
});
