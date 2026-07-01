import { describe, expect, it } from 'vitest';

import { formatBackendError, isBackendError } from './tauri-invoke';

describe('isBackendError', () => {
  it('accepts a well-formed { kind, message } object', () => {
    expect(isBackendError({ kind: 'locked', message: 'vault is locked' })).toBe(true);
  });

  it('accepts an object that also carries extra properties', () => {
    expect(isBackendError({ kind: 'x', message: 'y', detail: 1 })).toBe(true);
  });

  it('rejects null and undefined', () => {
    expect(isBackendError(null)).toBe(false);
    expect(isBackendError(undefined)).toBe(false);
  });

  it('rejects primitives', () => {
    expect(isBackendError('locked')).toBe(false);
    expect(isBackendError(42)).toBe(false);
    expect(isBackendError(true)).toBe(false);
  });

  it('rejects arrays', () => {
    expect(isBackendError([])).toBe(false);
    expect(isBackendError(['kind', 'message'])).toBe(false);
  });

  it('rejects objects missing either required key', () => {
    expect(isBackendError({})).toBe(false);
    expect(isBackendError({ kind: 'locked' })).toBe(false);
    expect(isBackendError({ message: 'oops' })).toBe(false);
  });

  it('rejects a plain Error instance (it has message but no kind)', () => {
    // This is why formatBackendError handles Error separately from BackendError.
    expect(isBackendError(new Error('network down'))).toBe(false);
  });

  it('checks key presence, not value type (undefined values still match)', () => {
    expect(isBackendError({ kind: undefined, message: undefined })).toBe(true);
  });
});

describe('formatBackendError', () => {
  it('strips the "validation: " prefix on validation errors', () => {
    const msg = formatBackendError({
      kind: 'validation',
      message: 'validation: title is required',
    });
    expect(msg).toBe('title is required');
  });

  it('leaves a validation message without prefix untouched', () => {
    const msg = formatBackendError({ kind: 'validation', message: 'bad input' });
    expect(msg).toBe('bad input');
  });

  it('maps locked to a friendly message', () => {
    expect(
      formatBackendError({ kind: 'locked', message: 'vault is locked' }),
    ).toBe('Vault is locked.');
  });

  it('maps wrong_password to a friendly message', () => {
    expect(
      formatBackendError({
        kind: 'wrong_password',
        message: 'incorrect master password',
      }),
    ).toBe('Incorrect master password.');
  });

  it('maps entry_not_found to "Entry not found." by default', () => {
    expect(
      formatBackendError({ kind: 'entry_not_found', message: 'entry not found' }),
    ).toBe('Entry not found.');
  });

  it('maps category_not_found to "Category not found." by default', () => {
    expect(
      formatBackendError({
        kind: 'category_not_found',
        message: 'category not found',
      }),
    ).toBe('Category not found.');
  });

  it('falls back to the backend message for unknown kinds', () => {
    expect(
      formatBackendError({ kind: 'something_else', message: 'boom' }),
    ).toBe('boom');
  });

  it('applies overrides ahead of the default mapping', () => {
    expect(
      formatBackendError(
        { kind: 'entry_not_found', message: 'entry not found' },
        { entry_not_found: 'Category not found.' },
      ),
    ).toBe('Category not found.');
  });

  it('allows overrides to replace validation prefix-stripping', () => {
    expect(
      formatBackendError(
        { kind: 'validation', message: 'validation: title is required' },
        { validation: 'Please check the form.' },
      ),
    ).toBe('Please check the form.');
  });

  it('returns Error.message for plain Error instances', () => {
    expect(formatBackendError(new Error('network down'))).toBe('network down');
  });

  it('coerces unknown values to a string', () => {
    expect(formatBackendError('weird')).toBe('weird');
    expect(formatBackendError(42)).toBe('42');
    expect(formatBackendError(null)).toBe('null');
  });
});
