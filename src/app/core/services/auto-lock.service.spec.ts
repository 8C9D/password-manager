import { describe, expect, it } from 'vitest';

import { countsAsActivity } from './auto-lock.service';

describe('countsAsActivity', () => {
  it('treats returning to the app as activity', () => {
    expect(countsAsActivity('visible')).toBe(true);
  });

  it('does not treat the window being hidden as activity', () => {
    // visibilitychange fires on both transitions. Counting the hide as
    // activity restarted the idle countdown exactly when the user stopped
    // watching, so an unattended vault got a fresh full timeout each time the
    // user tabbed away.
    expect(countsAsActivity('hidden')).toBe(false);
  });
});
