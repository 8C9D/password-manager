import { describe, expect, it } from 'vitest';

import { isTextEntryTarget, shortcutFor } from './vault-layout.component';

describe('shortcutFor', () => {
  it('maps the modifier chords to their actions', () => {
    expect(shortcutFor('k', true, false)).toBe('focus-search');
    expect(shortcutFor('n', true, false)).toBe('new-entry');
    expect(shortcutFor('l', true, false)).toBe('lock');
  });

  it('accepts the chords regardless of shift-capitalization', () => {
    expect(shortcutFor('K', true, false)).toBe('focus-search');
    expect(shortcutFor('L', true, false)).toBe('lock');
  });

  it('keeps the chords live while a field has focus', () => {
    // Lock especially: it must not become unreachable just because the cursor
    // is sitting in the notes box.
    expect(shortcutFor('l', true, true)).toBe('lock');
    expect(shortcutFor('n', true, true)).toBe('new-entry');
  });

  it('focuses search on a bare slash outside a text field', () => {
    expect(shortcutFor('/', false, false)).toBe('focus-search');
  });

  it('leaves a slash alone while the user is typing', () => {
    // Otherwise typing a URL into the form would jump focus to the search box.
    expect(shortcutFor('/', false, true)).toBeNull();
  });

  it('ignores unrelated keys', () => {
    expect(shortcutFor('a', false, false)).toBeNull();
    expect(shortcutFor('k', false, false)).toBeNull();
    expect(shortcutFor('Enter', true, false)).toBeNull();
  });
});

describe('isTextEntryTarget', () => {
  const el = (tagName: string, contentEditable = false) =>
    ({ tagName, isContentEditable: contentEditable }) as unknown as EventTarget;

  it('recognizes the editable form elements', () => {
    expect(isTextEntryTarget(el('INPUT'))).toBe(true);
    expect(isTextEntryTarget(el('TEXTAREA'))).toBe(true);
    expect(isTextEntryTarget(el('SELECT'))).toBe(true);
  });

  it('recognizes contenteditable hosts', () => {
    expect(isTextEntryTarget(el('DIV', true))).toBe(true);
  });

  it('rejects ordinary elements and a null target', () => {
    expect(isTextEntryTarget(el('DIV'))).toBe(false);
    expect(isTextEntryTarget(el('BUTTON'))).toBe(false);
    expect(isTextEntryTarget(null)).toBe(false);
  });
});
