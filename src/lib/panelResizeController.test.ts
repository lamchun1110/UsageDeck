import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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

import {
  PANEL_MAX_WIDTH,
  PANEL_MIN_WIDTH,
  PanelResizeController,
} from './panelResizeController.svelte';

/** Synthetic pointer press for calling controller handlers directly; jsdom lacks PointerEvent. */
function press(timeStamp: number, detail = 1) {
  return {
    button: 0,
    detail,
    timeStamp,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as PointerEvent;
}

/** A dragger element with the pointer-capture API the width drag relies on. */
function draggerElement() {
  const element = document.createElement('div');
  const captured = new Set<number>();
  element.setPointerCapture = (pointerId: number) => captured.add(pointerId);
  element.releasePointerCapture = (pointerId: number) => captured.delete(pointerId);
  element.hasPointerCapture = (pointerId: number) => captured.has(pointerId);
  return { element, captured };
}

/** Stub rAF onto the macrotask queue so coalesced width frames settle deterministically. */
function stubAnimationFrames() {
  vi.stubGlobal(
    'requestAnimationFrame',
    (callback: FrameRequestCallback) => setTimeout(callback, 0) as unknown as number,
  );
  vi.stubGlobal('cancelAnimationFrame', (handle: number) => clearTimeout(handle));
}

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
  afterEach(() => vi.unstubAllGlobals());

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

  it('restores automatic height on a quick second press, waiting for the active drag to settle', async () => {
    const controller = new PanelResizeController({
      platform: 'macos',
      scheduleFit: vi.fn(),
      onError: vi.fn(),
    });
    mocks.getPanelHeightMode.mockResolvedValue('automatic');
    // Hold the first drag's native begin step so the ordering can be observed.
    let releaseDrag!: (edge: string) => void;
    mocks.beginPanelResize.mockImplementationOnce(
      () => new Promise<string>((resolve) => (releaseDrag = resolve)),
    );

    controller.handleHeightPointerDown(press(100));
    expect(mocks.startResizeDragging).not.toHaveBeenCalled();

    // Within the 400ms window this is a double-press: it must toggle automatic
    // mode rather than start a second drag, and only once the first drag ends.
    controller.handleHeightPointerDown(press(200));
    expect(mocks.setPanelHeightAutomatic).not.toHaveBeenCalled();

    // The begin command runs after an await, so let microtasks flush before
    // releasing the held drag.
    await new Promise((resolve) => setTimeout(resolve, 0));
    releaseDrag('bottom');
    await vi.waitFor(() => expect(mocks.setPanelHeightAutomatic).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(controller.heightMode).toBe('automatic'));
    expect(mocks.startResizeDragging).toHaveBeenCalledTimes(1);
    expect(mocks.startResizeDragging).toHaveBeenCalledWith('South');
    // The native begin command persists manual height; the frontend only
    // mirrors that state, so the manual command is never issued here.
    expect(mocks.setPanelHeightManual).not.toHaveBeenCalled();
    expect(mocks.beginPanelResize).toHaveBeenCalledTimes(1);

    // The double-press re-arms the gesture: a later press starts a new drag
    // instead of toggling the mode again.
    controller.handleHeightPointerDown(press(250));
    await vi.waitFor(() => expect(mocks.beginPanelResize).toHaveBeenCalledTimes(2));
    expect(mocks.setPanelHeightAutomatic).toHaveBeenCalledTimes(1);
  });

  it('tracks pointer width drags, coalesces moves, and settles the resize axis', async () => {
    stubAnimationFrames();
    const controller = new PanelResizeController({
      platform: 'linux',
      scheduleFit: vi.fn(),
      onError: vi.fn(),
    });
    const { element: dragger, captured } = draggerElement();
    // A start width away from the field's 380 default, so observing it proves
    // the drag's async setup finished and the move listeners are attached.
    const startWidth = 400;
    mocks.currentPanelWidth.mockResolvedValue(startWidth);

    const pointerId = 7;
    controller.handleWidthPointerDown({
      ...press(0),
      pointerId,
      clientX: 100,
      currentTarget: dragger,
    } as unknown as PointerEvent & { clientX: number });
    await vi.waitFor(() => expect(controller.width).toBe(startWidth));
    expect(captured.has(pointerId)).toBe(true);

    // Both moves land before a frame fires, so they coalesce into one width:
    // 400 + (180 - 100) = 480.
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: 140 }));
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: 180 }));
    await new Promise((resolve) => setTimeout(resolve, 10));

    expect(controller.width).toBe(480);
    window.dispatchEvent(new Event('pointerup'));
    await vi.waitFor(() => expect(mocks.lockPanelResizeAxis).toHaveBeenCalledOnce());
    expect(captured.has(pointerId)).toBe(false);
    expect(mocks.setPanelWidth).toHaveBeenCalledTimes(1);
    expect(mocks.setPanelWidth).toHaveBeenCalledWith(480);
  });

  it('clamps pointer width drags to the panel bounds in both directions', async () => {
    stubAnimationFrames();
    const controller = new PanelResizeController({
      platform: 'linux',
      scheduleFit: vi.fn(),
      onError: vi.fn(),
    });
    const { element: dragger } = draggerElement();
    const startWidth = 400;
    mocks.currentPanelWidth.mockResolvedValue(startWidth);

    controller.handleWidthPointerDown({
      ...press(0),
      pointerId: 3,
      clientX: 100,
      currentTarget: dragger,
    } as unknown as PointerEvent & { clientX: number });
    await vi.waitFor(() => expect(controller.width).toBe(startWidth));

    // The frontend clamps the visible width state; the raw delta flows to the
    // backend, whose set_panel_width command clamps again before applying.
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: 9_999 }));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(controller.width).toBe(PANEL_MAX_WIDTH);
    expect(mocks.setPanelWidth).toHaveBeenCalled();

    window.dispatchEvent(new MouseEvent('pointermove', { clientX: -5_000 }));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(controller.width).toBe(PANEL_MIN_WIDTH);
    expect(mocks.setPanelWidth).toHaveBeenCalled();

    window.dispatchEvent(new Event('pointercancel'));
    await vi.waitFor(() => expect(mocks.lockPanelResizeAxis).toHaveBeenCalledOnce());
  });
});
