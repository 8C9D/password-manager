import { describe, expect, it } from 'vitest';

import { canCreateVault } from './vault.service';

describe('canCreateVault', () => {
  it('requires a master password of at least 8 characters', () => {
    expect(canCreateVault('short', 'short')).toBe(false);
    expect(canCreateVault('1234567', '1234567')).toBe(false);
    expect(canCreateVault('12345678', '12345678')).toBe(true);
  });

  it('requires the confirmation to match exactly', () => {
    expect(canCreateVault('longenough', 'different!')).toBe(false);
    expect(canCreateVault('longenough', 'longenough')).toBe(true);
    // Case- and whitespace-sensitive.
    expect(canCreateVault('longenough', 'LongEnough')).toBe(false);
    expect(canCreateVault('longenough', 'longenough ')).toBe(false);
  });

  it('rejects an empty confirmation', () => {
    expect(canCreateVault('longenough', '')).toBe(false);
  });
});
