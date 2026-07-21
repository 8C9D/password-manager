import { Injectable, signal } from '@angular/core';

import {
  EntryFull,
  EntryInput,
  EntryIssue,
  EntrySummary,
  GeneratedTotp,
  TotpUpdate,
  VaultHealth,
} from '../models/entry.model';
import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class PasswordEntryService {
  readonly entries = signal<EntrySummary[]>([]);
  readonly searchQuery = signal('');

  async list(): Promise<EntrySummary[]> {
    const rows = await call<EntrySummary[]>('list_entries');
    this.entries.set(rows);
    return rows;
  }

  async get(id: number): Promise<EntryFull> {
    return call<EntryFull>('get_entry', { id });
  }

  async create(input: EntryInput): Promise<number> {
    const id = await call<number>('create_entry', { input });
    await this.list();
    return id;
  }

  async update(id: number, input: EntryInput): Promise<void> {
    await call<void>('update_entry', { id, input });
    await this.list();
  }

  async remove(id: number): Promise<void> {
    await call<void>('delete_entry', { id });
    await this.list();
  }

  async generateTotp(id: number): Promise<GeneratedTotp> {
    return call<GeneratedTotp>('generate_totp', { id });
  }

  async auditVault(): Promise<VaultHealth> {
    return call<VaultHealth>('audit_vault');
  }

  async setFavorite(id: number, favorite: boolean): Promise<void> {
    await call<void>('set_favorite', { id, favorite });
    await this.list();
  }

  clear(): void {
    this.entries.set([]);
    this.searchQuery.set('');
  }
}

export function filterEntries(
  entries: readonly EntrySummary[],
  categoryId: number | null,
  query: string,
): EntrySummary[] {
  const q = query.trim().toLowerCase();
  return entries.filter((e) => {
    if (categoryId !== null && e.categoryId !== categoryId) return false;
    if (q === '') return true;
    return (
      e.title.toLowerCase().includes(q) ||
      e.username.toLowerCase().includes(q) ||
      e.urlOrAppName.toLowerCase().includes(q) ||
      e.tags.some((t) => t.toLowerCase().includes(q))
    );
  });
}

/** Split a comma-separated tag input into trimmed, non-empty tag strings. */
export function parseTagsInput(input: string): string[] {
  return input
    .split(',')
    .map((t) => t.trim())
    .filter((t) => t !== '');
}

export type EntrySortMode = 'title' | 'recently-used' | 'recently-created';

export function sortEntries(
  entries: readonly EntrySummary[],
  mode: EntrySortMode,
): EntrySummary[] {
  const byTitle = (a: EntrySummary, b: EntrySummary) =>
    a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });
  let cmp: (a: EntrySummary, b: EntrySummary) => number = byTitle;
  switch (mode) {
    case 'title':
      cmp = byTitle;
      break;
    case 'recently-used':
      // Never-used entries sink to the bottom, alphabetically.
      cmp = (a, b) => {
        if (a.lastUsedAt === null && b.lastUsedAt === null) return byTitle(a, b);
        if (a.lastUsedAt === null) return 1;
        if (b.lastUsedAt === null) return -1;
        return b.lastUsedAt.localeCompare(a.lastUsedAt) || byTitle(a, b);
      };
      break;
    case 'recently-created':
      cmp = (a, b) => b.createdAt.localeCompare(a.createdAt) || byTitle(a, b);
      break;
  }
  // Favorites always float to the top, then the chosen ordering applies.
  return [...entries].sort(
    (a, b) => Number(b.favorite) - Number(a.favorite) || cmp(a, b),
  );
}

/**
 * Returns a normalized URL string when the value parses as an absolute
 * http(s) URL, otherwise null.
 */
export function parseHttpUrl(value: string): string | null {
  const trimmed = value.trim();
  if (trimmed === '') return null;
  try {
    const url = new URL(trimmed);
    return url.protocol === 'http:' || url.protocol === 'https:'
      ? url.href
      : null;
  } catch {
    return null;
  }
}

/**
 * Group a numeric TOTP code into two halves for readability, e.g.
 * "287082" -> "287 082" and "12345678" -> "1234 5678".
 */
export function formatTotpCode(code: string): string {
  if (code.length < 2) return code;
  const mid = Math.ceil(code.length / 2);
  return `${code.slice(0, mid)} ${code.slice(mid)}`;
}

/**
 * Resolve the entry-form's TOTP inputs into the write action to send. A pending
 * removal wins; otherwise a non-blank secret sets/replaces; otherwise the field
 * is left untouched (undefined, which the backend treats as "keep").
 */
export function totpActionFrom(
  secret: string,
  removeExisting: boolean,
): TotpUpdate | undefined {
  if (removeExisting) return { action: 'clear' };
  const trimmed = secret.trim();
  if (trimmed !== '') return { action: 'set', value: trimmed };
  return undefined;
}

/** Human-readable badge labels for the problems flagged on an audited entry. */
export function describeIssue(issue: EntryIssue): string[] {
  const labels: string[] = [];
  if (issue.weak) labels.push('Weak');
  if (issue.reused) labels.push('Reused');
  if (issue.stale) labels.push('Old');
  return labels;
}

export interface EntryValidationResult {
  valid: boolean;
  errors: { field: 'title' | 'password'; message: string }[];
}

export function validateEntryInput(input: {
  title: string;
  password: string;
}): EntryValidationResult {
  const errors: EntryValidationResult['errors'] = [];
  if (input.title.trim() === '') {
    errors.push({ field: 'title', message: 'Title is required' });
  }
  if (input.password === '') {
    errors.push({ field: 'password', message: 'Password is required' });
  }
  return { valid: errors.length === 0, errors };
}
