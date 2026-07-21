import { Injectable, signal } from '@angular/core';

import {
  EntryFull,
  EntryInput,
  EntrySummary,
  GeneratedTotp,
  TotpUpdate,
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
      e.urlOrAppName.toLowerCase().includes(q)
    );
  });
}

export type EntrySortMode = 'title' | 'recently-used' | 'recently-created';

export function sortEntries(
  entries: readonly EntrySummary[],
  mode: EntrySortMode,
): EntrySummary[] {
  const byTitle = (a: EntrySummary, b: EntrySummary) =>
    a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });
  const sorted = [...entries];
  switch (mode) {
    case 'title':
      sorted.sort(byTitle);
      break;
    case 'recently-used':
      // Never-used entries sink to the bottom, alphabetically.
      sorted.sort((a, b) => {
        if (a.lastUsedAt === null && b.lastUsedAt === null) return byTitle(a, b);
        if (a.lastUsedAt === null) return 1;
        if (b.lastUsedAt === null) return -1;
        return b.lastUsedAt.localeCompare(a.lastUsedAt) || byTitle(a, b);
      });
      break;
    case 'recently-created':
      sorted.sort((a, b) => b.createdAt.localeCompare(a.createdAt) || byTitle(a, b));
      break;
  }
  return sorted;
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
