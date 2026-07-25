import { describe, expect, it, vi } from 'vitest';
import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';

import { EntryListComponent, describeBulkResult, retainVisible } from './entry-list.component';
import { CategoryService } from '../../core/services/category.service';
import { ConfirmService } from '../../core/services/confirm.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { EntrySummary } from '../../core/models/entry.model';

function entry(id: number): EntrySummary {
  return {
    id,
    categoryId: null,
    title: `Entry ${id}`,
    username: '',
    urlOrAppName: '',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    lastUsedAt: null,
    favorite: false,
    tags: [],
  };
}

describe('describeBulkResult', () => {
  it('reports a clean run in the plural', () => {
    expect(describeBulkResult('Moved', 3, 3)).toBe('Moved 3 entries.');
  });

  it('uses the singular for one entry', () => {
    expect(describeBulkResult('Trashed', 1, 1)).toBe('Trashed 1 entry.');
  });

  it('says so when the backend changed fewer than were selected', () => {
    // The count comes from the database, so it can be lower than the selection
    // when an id no longer names a live entry. Reporting the selection size
    // instead would misstate what happened to the vault.
    expect(describeBulkResult('Moved', 2, 5)).toBe(
      'Moved 2 of 5 selected entries; the rest were no longer there.',
    );
  });

  it('reports a run that changed nothing', () => {
    expect(describeBulkResult('Moved', 0, 4)).toBe(
      'Moved 0 of 4 selected entries; the rest were no longer there.',
    );
  });
});

describe('retainVisible', () => {
  it('keeps only entries still on screen', () => {
    // Searching or switching category hides entries; a bulk action must not
    // reach one the user can no longer see.
    const kept = retainVisible(new Set([1, 2, 3]), [entry(1), entry(3)]);
    expect([...kept].sort()).toEqual([1, 3]);
  });

  it('empties the selection when nothing is visible', () => {
    expect(retainVisible(new Set([1, 2]), []).size).toBe(0);
  });

  it('leaves a fully visible selection alone', () => {
    const kept = retainVisible(new Set([1, 2]), [entry(1), entry(2)]);
    expect(kept.size).toBe(2);
  });
});

describe('EntryListComponent selection', () => {
  interface Internals {
    selecting: { (): boolean; set(v: boolean): void };
    selected: { (): ReadonlySet<number> };
    selectedIds: () => number[];
    toggleSelecting: () => void;
    toggleAll: () => void;
    bulkNotice: { (): string | null; set(v: string | null): void };
  }

  function setup(rows: EntrySummary[]) {
    const entries = signal<EntrySummary[]>(rows);
    const searchQuery = signal('');
    TestBed.configureTestingModule({
      imports: [EntryListComponent],
      providers: [
        {
          provide: PasswordEntryService,
          useValue: {
            entries,
            searchQuery,
            list: vi.fn(async () => rows),
            setEntriesCategory: vi.fn(async () => 0),
            setEntriesFavorite: vi.fn(async () => 0),
            removeEntries: vi.fn(async () => 0),
          },
        },
        {
          provide: CategoryService,
          useValue: { categories: () => [], selected: () => null, list: vi.fn(async () => []) },
        },
        { provide: ConfirmService, useValue: { ask: vi.fn(async () => true) } },
      ],
    });
    const fixture = TestBed.createComponent(EntryListComponent);
    fixture.detectChanges();
    return {
      fixture,
      searchQuery,
      entries,
      component: fixture.componentInstance as unknown as Internals,
    };
  }

  it('drops entries from the selection once a search hides them, and does not bring them back', () => {
    // Masking the selection instead of pruning it meant clearing the search
    // silently restored entries the user had narrowed away - and the next bulk
    // action would then reach entries they never meant to pick.
    const { fixture, searchQuery, component } = setup([entry(1), entry(2), entry(3)]);
    component.toggleSelecting();
    component.toggleAll();
    fixture.detectChanges();
    expect(component.selectedIds()).toHaveLength(3);

    searchQuery.set('Entry 2');
    fixture.detectChanges();
    expect(component.selectedIds()).toEqual([2]);

    searchQuery.set('');
    fixture.detectChanges();
    expect(component.selectedIds()).toEqual([2]);

    fixture.destroy();
  });

  it('leaves selection mode when nothing is left to select', () => {
    // A bulk trash can empty the list. The controls for leaving selection mode
    // live inside the non-empty branch, so staying in it would strand the user
    // in a mode with no visible way out.
    const { fixture, entries, component } = setup([entry(1)]);
    component.toggleSelecting();
    fixture.detectChanges();
    expect(component.selecting()).toBe(true);

    entries.set([]);
    fixture.detectChanges();
    expect(component.selecting()).toBe(false);

    fixture.destroy();
  });

  it('keeps the result notice visible after the list empties', () => {
    const { fixture, entries, component } = setup([entry(1)]);
    component.bulkNotice.set('Trashed 1 entry.');
    entries.set([]);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Trashed 1 entry.');
    fixture.destroy();
  });
});
