import { describe, expect, it } from 'vitest';

import { canCreateVault, masterPasswordLength } from './vault.service';

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

describe('masterPasswordLength', () => {
  it('counts characters, not UTF-16 code units', () => {
    // The backend counts `chars()`. A plain `.length` would call four emoji
    // eight characters and let through a password the backend then rejects.
    expect(masterPasswordLength('12345678')).toBe(8);
    expect(masterPasswordLength('\u{1F600}\u{1F600}\u{1F600}\u{1F600}')).toBe(4);
    expect(masterPasswordLength('日本語パスワ')).toBe(6);
  });
});

describe('canCreateVault with non-ASCII passwords', () => {
  it('rejects a password that is long in bytes but short in characters', () => {
    expect(canCreateVault('日本語パスワ', '日本語パスワ')).toBe(false);
    expect(canCreateVault('日本語パスワード', '日本語パスワード')).toBe(true);
  });

  it('rejects four emoji, which .length would have called eight characters', () => {
    const four = '\u{1F600}\u{1F600}\u{1F600}\u{1F600}';
    expect(four.length).toBe(8);
    expect(canCreateVault(four, four)).toBe(false);
  });
});
