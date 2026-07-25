import { beforeEach, describe, expect, it, vi } from 'vitest';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { provideLocationMocks } from '@angular/common/testing';
import { TestBed } from '@angular/core/testing';

import { EntryFormComponent } from './entry-form.component';
import { CategoryService } from '../../core/services/category.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { EntryFull } from '../../core/models/entry.model';

function entry(id: number, over: Partial<EntryFull> = {}): EntryFull {
  return {
    id,
    categoryId: null,
    title: `Entry ${id}`,
    username: 'alice',
    urlOrAppName: 'example.com',
    password: 'hunter2',
    notes: 'private',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    lastUsedAt: null,
    hasTotp: true,
    favorite: true,
    tags: ['work'],
    passwordExpiryDays: 90,
    passwordDueAt: '2026-04-01T00:00:00Z',
    ...over,
  };
}

/** The form's fields are `protected`; tests read them off the instance. */
interface FormFields {
  title: string;
  username: string;
  password: string;
  notes: string;
  tagsInput: string;
  favorite: boolean;
  hasExistingTotp: boolean;
  passwordExpiryDays: number | string;
  editingId: () => number | null;
  duplicating: () => boolean;
}

/** Let the component's async route handler finish, then re-render. */
async function settle(harness: RouterTestingHarness): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  harness.detectChanges();
}

describe('EntryFormComponent', () => {
  let get: ReturnType<typeof vi.fn>;
  let create: ReturnType<typeof vi.fn>;
  let update: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    get = vi.fn(async (id: number) => entry(id));
    create = vi.fn(async () => 7);
    update = vi.fn(async () => undefined);

    TestBed.configureTestingModule({
      providers: [
        provideRouter([
          { path: 'vault/new', component: EntryFormComponent },
          { path: 'vault/:id/edit', component: EntryFormComponent },
        ]),
        provideLocationMocks(),
        { provide: PasswordEntryService, useValue: { get, create, update } },
        {
          provide: CategoryService,
          useValue: { list: vi.fn(async () => []), categories: () => [] },
        },
      ],
    });
  });

  it('prefills a duplicate from its source without carrying the 2FA secret', async () => {
    const harness = await RouterTestingHarness.create();
    const form = (await harness.navigateByUrl(
      '/vault/new?duplicate=5',
    )) as unknown as FormFields;
    await settle(harness);

    expect(get).toHaveBeenCalledWith(5);
    expect(form.title).toBe('Entry 5 (copy)');
    expect(form.password).toBe('hunter2');
    expect(form.tagsInput).toBe('work');
    expect(form.passwordExpiryDays).toBe(90);
    // get_entry reports only that a secret exists, never the secret, so a copy
    // cannot carry one.
    expect(form.hasExistingTotp).toBe(false);
    // The source is a template, not a row to overwrite.
    expect(form.editingId()).toBeNull();
    expect(form.duplicating()).toBe(true);
  });

  it('starts blank when leaving a duplicate for a plain new entry', async () => {
    // Ctrl+N from /vault/new?duplicate=5 goes to /vault/new: the same route
    // config, so Angular reuses the component and ngOnInit does not run again.
    // Without a reset the form still holds the copied entry, and saving the
    // "new" entry silently writes another copy of it.
    const harness = await RouterTestingHarness.create();
    await harness.navigateByUrl('/vault/new?duplicate=5');
    await settle(harness);
    const form = (await harness.navigateByUrl('/vault/new')) as unknown as FormFields;
    await settle(harness);

    expect(form.title).toBe('');
    expect(form.password).toBe('');
    expect(form.username).toBe('');
    expect(form.notes).toBe('');
    expect(form.tagsInput).toBe('');
    expect(form.favorite).toBe(false);
    expect(form.passwordExpiryDays).toBe('');
    expect(form.duplicating()).toBe(false);
    expect(form.editingId()).toBeNull();
  });

  it('loads the entry named by a later edit navigation, not the earlier one', async () => {
    const harness = await RouterTestingHarness.create();
    await harness.navigateByUrl('/vault/1/edit');
    await settle(harness);
    const form = (await harness.navigateByUrl('/vault/2/edit')) as unknown as FormFields;
    await settle(harness);

    expect(form.editingId()).toBe(2);
    expect(form.title).toBe('Entry 2');
  });

  it('refuses a garbage edit id instead of offering an empty form', async () => {
    const harness = await RouterTestingHarness.create();
    const form = (await harness.navigateByUrl('/vault/0/edit')) as unknown as FormFields;
    await settle(harness);

    expect(get).not.toHaveBeenCalled();
    expect(form.editingId()).toBeNull();
    expect(harness.routeNativeElement?.textContent).toContain('Invalid entry id.');
  });

  it('ignores a duplicate source that is not a positive integer', async () => {
    const harness = await RouterTestingHarness.create();
    const form = (await harness.navigateByUrl(
      '/vault/new?duplicate=nonsense',
    )) as unknown as FormFields;
    await settle(harness);

    expect(get).not.toHaveBeenCalled();
    expect(form.title).toBe('');
    expect(form.duplicating()).toBe(false);
  });
});
