import { describe, expect, it } from 'vitest';

import { EntrySummary } from '../models/entry.model';
import {
  describeDue,
  describeIssue,
  filterEntries,
  formatTotpCode,
  MAX_EXPIRY_DAYS,
  parseExpiryDays,
  parseHttpUrl,
  parseTagsInput,
  sortEntries,
  totpActionFrom,
  validateEntryInput,
} from './password-entry.service';

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
    favorite: false,
    tags: [],
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

  it('treats a whitespace-only query as no query (returns everything)', () => {
    expect(filterEntries(SAMPLE, null, '     ')).toHaveLength(4);
  });

  it('whitespace-only query still respects an active category filter', () => {
    expect(filterEntries(SAMPLE, 10, '   ').map((e) => e.id)).toEqual([1, 3]);
  });

  it('returns empty for an empty entries array regardless of filters', () => {
    expect(filterEntries([], null, '')).toEqual([]);
    expect(filterEntries([], 10, 'github')).toEqual([]);
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

describe('sortEntries', () => {
  const list: EntrySummary[] = [
    makeEntry({
      id: 1,
      title: 'banana',
      createdAt: '2026-01-02T00:00:00Z',
      lastUsedAt: '2026-03-01T00:00:00Z',
    }),
    makeEntry({
      id: 2,
      title: 'Apple',
      createdAt: '2026-01-03T00:00:00Z',
      lastUsedAt: null,
    }),
    makeEntry({
      id: 3,
      title: 'cherry',
      createdAt: '2026-01-01T00:00:00Z',
      lastUsedAt: '2026-04-01T00:00:00Z',
    }),
  ];

  it('sorts by title case-insensitively', () => {
    expect(sortEntries(list, 'title').map((e) => e.id)).toEqual([2, 1, 3]);
  });

  it('sorts by most recently used, never-used entries last', () => {
    expect(sortEntries(list, 'recently-used').map((e) => e.id)).toEqual([3, 1, 2]);
  });

  it('sorts by most recently created', () => {
    expect(sortEntries(list, 'recently-created').map((e) => e.id)).toEqual([2, 1, 3]);
  });

  it('does not mutate the input array', () => {
    const before = list.map((e) => e.id);
    sortEntries(list, 'recently-used');
    expect(list.map((e) => e.id)).toEqual(before);
  });
});

describe('parseHttpUrl', () => {
  it('accepts absolute http and https URLs', () => {
    expect(parseHttpUrl('https://github.com/login')).toBe('https://github.com/login');
    expect(parseHttpUrl('http://localhost:8080')).toBe('http://localhost:8080/');
  });

  it('trims surrounding whitespace', () => {
    expect(parseHttpUrl('  https://example.com  ')).toBe('https://example.com/');
  });

  it('rejects bare hostnames and app names', () => {
    expect(parseHttpUrl('github.com')).toBeNull();
    expect(parseHttpUrl('1Password desktop app')).toBeNull();
    expect(parseHttpUrl('')).toBeNull();
  });

  it('rejects non-http schemes', () => {
    expect(parseHttpUrl('file:///etc/passwd')).toBeNull();
    expect(parseHttpUrl('javascript:alert(1)')).toBeNull();
    expect(parseHttpUrl('ftp://example.com')).toBeNull();
  });
});

describe('formatTotpCode', () => {
  it('splits a 6-digit code into two groups of three', () => {
    expect(formatTotpCode('287082')).toBe('287 082');
  });

  it('splits an 8-digit code into two groups of four', () => {
    expect(formatTotpCode('12345678')).toBe('1234 5678');
  });

  it('leaves a very short code unchanged', () => {
    expect(formatTotpCode('7')).toBe('7');
    expect(formatTotpCode('')).toBe('');
  });
});

describe('totpActionFrom', () => {
  it('returns a clear action when removal is requested (winning over a secret)', () => {
    expect(totpActionFrom('', true)).toEqual({ action: 'clear' });
    expect(totpActionFrom('JBSWY3DPEHPK3PXP', true)).toEqual({ action: 'clear' });
  });

  it('returns a trimmed set action for a non-blank secret', () => {
    expect(totpActionFrom('  JBSWY3DPEHPK3PXP  ', false)).toEqual({
      action: 'set',
      value: 'JBSWY3DPEHPK3PXP',
    });
  });

  it('returns undefined (keep) when the secret is blank and nothing is removed', () => {
    expect(totpActionFrom('', false)).toBeUndefined();
    expect(totpActionFrom('   ', false)).toBeUndefined();
  });
});

describe('describeIssue', () => {
  const base = { id: 1, title: 'X', weak: false, reused: false, stale: false, due: false };

  it('lists only the flagged problems, in a stable order', () => {
    expect(describeIssue({ ...base, weak: true, reused: true })).toEqual([
      'Weak',
      'Reused',
    ]);
    expect(describeIssue({ ...base, stale: true })).toEqual(['Old']);
    expect(
      describeIssue({ ...base, weak: true, reused: true, stale: true }),
    ).toEqual(['Weak', 'Reused', 'Old']);
    expect(describeIssue({ ...base, due: true })).toEqual(['Due']);
    expect(
      describeIssue({ ...base, weak: true, reused: true, stale: true, due: true }),
    ).toEqual(['Weak', 'Reused', 'Old', 'Due']);
  });

  it('returns an empty list when nothing is wrong', () => {
    expect(describeIssue(base)).toEqual([]);
  });
});

describe('parseTagsInput', () => {
  it('splits, trims, and drops empty tags', () => {
    expect(parseTagsInput('work,  personal ,,  , email')).toEqual([
      'work',
      'personal',
      'email',
    ]);
    expect(parseTagsInput('   ')).toEqual([]);
    expect(parseTagsInput('')).toEqual([]);
  });
});

describe('filterEntries with tags', () => {
  it('matches entries by tag as well as title/username/url', () => {
    const entries = [
      makeEntry({ id: 1, title: 'GitHub', tags: ['dev', 'work'] }),
      makeEntry({ id: 2, title: 'Bank', tags: ['finance'] }),
    ];
    expect(filterEntries(entries, null, 'work').map((e) => e.id)).toEqual([1]);
  });
});

describe('sortEntries favorites', () => {
  it('floats favorites to the top within the chosen order', () => {
    const entries = [
      makeEntry({ id: 1, title: 'Apple', favorite: false }),
      makeEntry({ id: 2, title: 'Zebra', favorite: true }),
      makeEntry({ id: 3, title: 'Mango', favorite: false }),
    ];
    expect(sortEntries(entries, 'title').map((e) => e.title)).toEqual([
      'Zebra',
      'Apple',
      'Mango',
    ]);
  });
});

describe('parseExpiryDays', () => {
  it('turns a positive whole number of days into a reminder', () => {
    expect(parseExpiryDays(90)).toBe(90);
    expect(parseExpiryDays('30')).toBe(30);
  });

  it('treats blank, zero, and negatives as no reminder', () => {
    expect(parseExpiryDays('')).toBeNull();
    expect(parseExpiryDays(0)).toBeNull();
    expect(parseExpiryDays(-5)).toBeNull();
    expect(parseExpiryDays('not a number')).toBeNull();
  });

  it('truncates fractions and clamps to the backend maximum', () => {
    expect(parseExpiryDays(90.7)).toBe(90);
    expect(parseExpiryDays(MAX_EXPIRY_DAYS + 100)).toBe(MAX_EXPIRY_DAYS);
  });
});

describe('describeDue', () => {
  const now = new Date('2026-07-01T12:00:00Z');
  const inDays = (n: number) =>
    new Date(now.getTime() + n * 86_400_000).toISOString();

  it('reports nothing when there is no reminder', () => {
    expect(describeDue(null, now)).toBeNull();
  });

  it('counts down to a future due date', () => {
    expect(describeDue(inDays(10), now)).toEqual({ text: 'Due in 10 days', overdue: false });
    expect(describeDue(inDays(1), now)).toEqual({ text: 'Due in 1 day', overdue: false });
  });

  it('marks today and anything past it as overdue', () => {
    expect(describeDue(inDays(0), now)).toEqual({ text: 'Due today', overdue: true });
    expect(describeDue(inDays(-1), now)).toEqual({ text: 'Overdue by 1 day', overdue: true });
    expect(describeDue(inDays(-45), now)).toEqual({ text: 'Overdue by 45 days', overdue: true });
  });

  it('reports nothing for an unparseable date rather than a wrong countdown', () => {
    expect(describeDue('not a date', now)).toBeNull();
  });
});
