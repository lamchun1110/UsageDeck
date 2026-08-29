import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  beginPanelResize,
  currentPanelWidth,
  getPanelHeightMode,
  getPanelResizeEdge,
  lockPanelResizeAxis,
  setPanelHeightAutomatic,
  setPanelHeightManual,
  setPanelWidth,
  type PanelHeightMode,
  type PanelResizeEdge,
} from './backend';
import { t } from './i18n.svelte';
import type { DesktopPlatform } from './platform';

export const PANEL_MIN_WIDTH = 320;
export const PANEL_MAX_WIDTH = 560;
const PANEL_WIDTH_STEP = 16;

interface PanelResizeControllerOptions {
  platform: DesktopPlatform;
  scheduleFit: () => void;
  onError: (message: string) => void;
}

function clampPanelWidth(width: number) {
  return Math.min(PANEL_MAX_WIDTH, Math.max(PANEL_MIN_WIDTH, Math.round(width)));
}

function tauriAvailable() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export class PanelResizeController {
  edge = $state<PanelResizeEdge>('bottom');
  heightMode = $state<PanelHeightMode>('automatic');
  width = $state(380);

  #heightModeRequest = 0;
  #heightModeMutation: Promise<void> = Promise.resolve();
  #lastResizeGripPointerAt = Number.NEGATIVE_INFINITY;
  #panelResizeOperation: Promise<void> | null = null;
  #cancelWidthDrag: (() => void) | null = null;

  constructor(private readonly options: PanelResizeControllerOptions) {
    this.edge = options.platform === 'windows' ? 'top' : 'bottom';
  }

  refresh() {
    this.refreshEdge();
    this.refreshHeightMode();
  }

  refreshEdge() {
    if (!tauriAvailable()) return;
    void getPanelResizeEdge()
      .then((edge) => (this.edge = edge))
      .catch(() => undefined);
  }

  refreshHeightMode() {
    if (!tauriAvailable()) return;
    const request = ++this.#heightModeRequest;
    void getPanelHeightMode()
      .then((mode) => {
        if (request !== this.#heightModeRequest) return;
        this.heightMode = mode;
        if (mode === 'automatic') this.options.scheduleFit();
      })
      .catch(() => undefined);
  }

  waitForHeightModeMutation() {
    return this.#heightModeMutation;
  }

  handleHeightPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const pointerAt = event.timeStamp;
    const repeatedPress = event.detail > 1 || pointerAt - this.#lastResizeGripPointerAt <= 400;
    this.#lastResizeGripPointerAt = repeatedPress ? Number.NEGATIVE_INFINITY : pointerAt;
    if (repeatedPress) {
      const activeResize = this.#panelResizeOperation;
      void (async () => {
        if (activeResize) await activeResize;
        await this.changeHeightMode('automatic');
      })();
      return;
    }

    const operation = (async () => {
      try {
        await this.#heightModeMutation;
        const edge = await beginPanelResize();
        this.edge = edge;
        // The native begin command has already persisted the current height as manual. Mirroring it
        // here stops any in-flight frontend auto-fit without waiting for the first resize event.
        this.#acceptHeightMode('manual');
        // TODO(macOS): Tao 0.35 reports native resize dragging as unsupported and Tauri currently
        // swallows that runtime error. Re-test after Tauri/Tao upgrades; add an AppKit fallback if
        // upstream support is still unavailable.
        await getCurrentWindow().startResizeDragging(edge === 'top' ? 'North' : 'South');
      } catch {
        this.options.onError(t('app.error.resizeStart'));
      } finally {
        await lockPanelResizeAxis().catch(() => undefined);
        this.refreshHeightMode();
      }
    })();
    this.#panelResizeOperation = operation;
    void operation.finally(() => {
      if (this.#panelResizeOperation === operation) this.#panelResizeOperation = null;
    });
  }

  handleWidthPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const dragger = event.currentTarget as HTMLElement;
    dragger.setPointerCapture(event.pointerId);
    void (async () => {
      try {
        // Manual pointer-tracked resize: programmatic setSize on each move. Unlike the native
        // startResizeDragging gesture (unreliable for borderless windows), this works everywhere.
        const startWidth = await currentPanelWidth();
        this.width = clampPanelWidth(startWidth);
        const startX = event.clientX;
        let latestWidth = startWidth;
        let animationFrame: number | null = null;
        let resizeOperation = Promise.resolve();
        const queueLatestWidth = () => {
          if (animationFrame !== null) return;
          animationFrame = requestAnimationFrame(() => {
            animationFrame = null;
            const width = latestWidth;
            resizeOperation = resizeOperation
              .then(() => setPanelWidth(width))
              .catch(() => undefined);
          });
        };
        const onMove = (moveEvent: PointerEvent) => {
          latestWidth = startWidth + (moveEvent.clientX - startX);
          this.width = clampPanelWidth(latestWidth);
          queueLatestWidth();
        };
        const finish = () => {
          window.removeEventListener('pointermove', onMove);
          window.removeEventListener('pointerup', finish);
          window.removeEventListener('pointercancel', finish);
          this.#cancelWidthDrag = null;
          if (dragger.hasPointerCapture(event.pointerId)) {
            dragger.releasePointerCapture(event.pointerId);
          }
          if (animationFrame !== null) {
            cancelAnimationFrame(animationFrame);
            animationFrame = null;
            const width = latestWidth;
            resizeOperation = resizeOperation
              .then(() => setPanelWidth(width))
              .catch(() => undefined);
          }
          void resizeOperation.finally(() => lockPanelResizeAxis().catch(() => undefined));
        };
        this.#cancelWidthDrag = finish;
        window.addEventListener('pointermove', onMove);
        window.addEventListener('pointerup', finish);
        window.addEventListener('pointercancel', finish);
      } catch {
        this.options.onError(t('app.error.widthResize'));
      }
    })();
  }

  /** Detaches an in-flight width drag (window hidden or surface removed) so
   * its listeners cannot keep resizing the panel from stray pointer moves. */
  cancelWidthDrag() {
    this.#cancelWidthDrag?.();
    this.#cancelWidthDrag = null;
  }

  async handleWidthKeydown(event: KeyboardEvent) {
    if (!['Home', 'End', 'ArrowLeft', 'ArrowRight'].includes(event.key)) return;

    event.preventDefault();
    event.stopPropagation();
    try {
      let target: number;
      if (event.key === 'Home') target = PANEL_MIN_WIDTH;
      else if (event.key === 'End') target = PANEL_MAX_WIDTH;
      else {
        const currentWidth = clampPanelWidth(await currentPanelWidth());
        const direction = event.key === 'ArrowLeft' ? -1 : 1;
        target = currentWidth + direction * PANEL_WIDTH_STEP * (event.shiftKey ? 2 : 1);
      }
      this.width = clampPanelWidth(target);
      await setPanelWidth(this.width);
      await lockPanelResizeAxis();
    } catch {
      this.options.onError(t('app.error.widthResize'));
    }
  }

  async changeHeightMode(mode: PanelHeightMode) {
    if (!tauriAvailable()) return;
    const request = ++this.#heightModeRequest;
    const operation = this.#heightModeMutation.then(() =>
      mode === 'automatic' ? setPanelHeightAutomatic() : setPanelHeightManual(),
    );
    this.#heightModeMutation = operation.catch(() => undefined);
    try {
      await operation;
      if (request === this.#heightModeRequest) this.refreshHeightMode();
    } catch {
      if (request !== this.#heightModeRequest) return;
      this.options.onError(t('app.error.heightMode'));
      this.refreshHeightMode();
    }
  }

  #acceptHeightMode(mode: PanelHeightMode) {
    this.#heightModeRequest += 1;
    this.heightMode = mode;
  }
}
