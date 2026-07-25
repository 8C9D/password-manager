import { describe, expect, it } from 'vitest';

import { trapTabFocus } from './confirm-dialog.component';

describe('trapTabFocus', () => {
  const cancel = { id: 'cancel' } as unknown as HTMLElement;
  const confirm = { id: 'confirm' } as unknown as HTMLElement;
  const focusable = [cancel, confirm];

  it('wraps forward from the last element to the first', () => {
    expect(trapTabFocus(focusable, confirm, false)).toBe(cancel);
  });

  it('wraps backward from the first element to the last', () => {
    expect(trapTabFocus(focusable, cancel, true)).toBe(confirm);
  });

  it('leaves interior moves to the browser', () => {
    expect(trapTabFocus(focusable, cancel, false)).toBeNull();
    expect(trapTabFocus(focusable, confirm, true)).toBeNull();
  });

  it('pulls focus back in when it has escaped the dialog', () => {
    // Focus parked outside the dialog is neither first nor last, so a plain
    // Tab is left alone; Shift+Tab likewise. The wrap only has to hold at the
    // two edges, which is where escape actually happens.
    expect(trapTabFocus(focusable, null, false)).toBeNull();
  });

  it('does nothing when the dialog has no focusable children', () => {
    expect(trapTabFocus([], cancel, false)).toBeNull();
    expect(trapTabFocus([], cancel, true)).toBeNull();
  });
});
