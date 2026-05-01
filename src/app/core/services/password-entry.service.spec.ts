import { describe, expect, it } from 'vitest';

import { EntrySummary } from '../models/entry.model';
import { filterEntries, validateEntryInput } from './password-entry.service';

function makeEntry(overrides: Partial<EntrySummary>): EntrySummary {
  return {
    id: overrides.id ?? 1,
    categoryId: overrides.categoryId ?? null,
    title: overrides.title ?? 'Entry',
    username: overrides.username ?? '',
    urlOrAppName: overrides.urlOrAppName ?? '',
    createdAt: '2026-05-17T00:00:00Z',
    updatedAt: '2026-05-17T00:00:00Z',
    lastUsedAt: null,
    ...overrides,
  };
}

const SAMPLE: EntrySummary[] = [
  makeEntry({ id: 1, title: 'GitHub', username: 'alice', urlOrAppName: 'github.com', categoryId: 10 }),
  makeEntry({ id: 2, title: 'Bank', username: 'alice@example.com', urlOrAppName: 'mybank.example.com', categoryId: 20 }),
  makeEntry({ id: 3, title: 'Email', username: 'bob', urlOrAppName: 'mail.example.com', categoryId: 10 }),
  makeEntry({ id: 4, title: 'Notes app', username: '', urlOrAppName: '', categoryId: null }),
];

describe('filterEntries', () => {
  it('returns everything when no filters applied', () => {
    expect(filterEntries(SAMPLE, null, '')).toHaveLength(4);
  });

  it('filters by category id only', () => {
    const result = filterEntries(SAMPLE, 10, '');
    expect(result.map((e) => e.id)).toEqual([1, 3]);
  });

  it('matches title substring case-insensitively', () => {
    expect(filterEntries(SAMPLE, null, 'github').map((e) => e.id)).toEqual([1]);
    expect(filterEntries(SAMPLE, null, 'GITHUB').map((e) => e.id)).toEqual([1]);
  });

  it('matches username substring', () => {
    expect(filterEntries(SAMPLE, null, 'alice').map((e) => e.id).sort()).toEqual([1, 2]);
  });

  it('matches url substring', () => {
    expect(filterEntries(SAMPLE, null, 'mail.example').map((e) => e.id)).toEqual([3]);
  });

  it('combines category + query filters with AND', () => {
    const result = filterEntries(SAMPLE, 10, 'alice');
    expect(result.map((e) => e.id)).toEqual([1]);
  });

  it('trims whitespace from query', () => {
    expect(filterEntries(SAMPLE, null, '   github   ').map((e) => e.id)).toEqual([1]);
  });

  it('returns empty when nothing matches', () => {
    expect(filterEntries(SAMPLE, null, 'nope-no-match-zzz')).toEqual([]);
  });

  it('excludes entries with categoryId=null when filtering by category', () => {
    const result = filterEntries(SAMPLE, 10, '');
    expect(result.find((e) => e.id === 4)).toBeUndefined();
  });
});

describe('validateEntryInput', () => {
  it('accepts a fully populated input', () => {
    const result = validateEntryInput({ title: 'GitHub', password: 'hunter2' });
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  it('rejects empty title', () => {
    const result = validateEntryInput({ title: '', password: 'pw' });
    expect(result.valid).toBe(false);
    expect(result.errors).toEqual([{ field: 'title', message: 'Title is required' }]);
  });

  it('rejects whitespace-only title', () => {
    const result = validateEntryInput({ title: '   ', password: 'pw' });
    expect(result.valid).toBe(false);
    expect(result.errors[0]?.field).toBe('title');
  });

  it('rejects empty password', () => {
    const result = validateEntryInput({ title: 'X', password: '' });
    expect(result.valid).toBe(false);
    expect(result.errors).toEqual([{ field: 'password', message: 'Password is required' }]);
  });

  it('returns both errors when title and password are empty', () => {
    const result = validateEntryInput({ title: '', password: '' });
    expect(result.valid).toBe(false);
    expect(result.errors).toHaveLength(2);
  });

  it('accepts password that is whitespace (intentional — user might pick odd values)', () => {
    const result = validateEntryInput({ title: 'X', password: ' ' });
    expect(result.valid).toBe(true);
  });
});
