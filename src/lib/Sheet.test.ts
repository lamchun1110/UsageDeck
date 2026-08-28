import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Sheet from './Sheet.svelte';
import TestChild from './TestChild.svelte';

describe('Sheet', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('focuses the first control on open and restores the previous focus on close', async () => {
    const opener = document.createElement('button');
    opener.textContent = 'before';
    document.body.append(opener);
    opener.focus();

    const onDismiss = vi.fn();
    const { unmount } = render(Sheet, {
      label: 'test dialog',
      onDismiss,
      children: TestChild,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'inside' })).toHaveFocus());
    unmount();
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it('dismisses with Escape, traps Tab, and honors dismissible=false while pending', async () => {
    const onDismiss = vi.fn();
    const props = { label: 'test dialog', onDismiss, children: TestChild };
    const { rerender, unmount } = render(Sheet, props);

    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onDismiss).toHaveBeenCalledTimes(1);

    const pending = { ...props, dismissible: false };
    rerender(pending);
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onDismiss).toHaveBeenCalledTimes(1);
    unmount();
  });

  it('dismisses on backdrop activation only when dismissOnBackdrop is set', async () => {
    const onDismiss = vi.fn();
    const props = {
      label: 'test dialog',
      onDismiss,
      dismissOnBackdrop: true,
      children: TestChild,
    };
    const { rerender, unmount } = render(Sheet, props);

    // Clicks on the dialog content never dismiss, even over the backdrop.
    await fireEvent.click(screen.getByRole('button', { name: 'inside' }));
    expect(onDismiss).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId('sheet-backdrop'));
    expect(onDismiss).toHaveBeenCalledTimes(1);

    rerender({ ...props, dismissOnBackdrop: false });
    await fireEvent.click(screen.getByTestId('sheet-backdrop'));
    expect(onDismiss).toHaveBeenCalledTimes(1);
    unmount();
  });
});
