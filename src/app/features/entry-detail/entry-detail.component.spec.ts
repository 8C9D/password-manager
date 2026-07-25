import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BehaviorSubject } from 'rxjs';
import { ActivatedRoute, convertToParamMap, ParamMap, Router } from '@angular/router';
import { TestBed } from '@angular/core/testing';

import { EntryDetailComponent } from './entry-detail.component';
import { CategoryService } from '../../core/services/category.service';
import { ClipboardService } from '../../core/services/clipboard.service';
import { ConfirmService } from '../../core/services/confirm.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { EntryFull, GeneratedTotp } from '../../core/models/entry.model';

/**
 * A promise whose resolution this test controls, so a component's in-flight
 * request can be held open across another navigation. The stale-response guards
 * only misbehave while a request is still pending, which real timing reproduces
 * unreliably.
 */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function entry(id: number, over: Partial<EntryFull> = {}): EntryFull {
  return {
    id,
    categoryId: null,
    title: `Entry ${id}`,
    username: 'alice',
    urlOrAppName: 'example.com',
    password: 'hunter2',
    notes: null,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    lastUsedAt: null,
    hasTotp: false,
    favorite: false,
    tags: [],
    passwordExpiryDays: null,
    passwordDueAt: null,
    fields: [],
    ...over,
  };
}

describe('EntryDetailComponent', () => {
  let params: BehaviorSubject<ParamMap>;
  let getCalls: { id: number; d: ReturnType<typeof deferred<EntryFull>> }[];
  let totpCalls: { id: number; d: ReturnType<typeof deferred<GeneratedTotp>> }[];

  beforeEach(() => {
    params = new BehaviorSubject<ParamMap>(convertToParamMap({ id: '1' }));
    getCalls = [];
    totpCalls = [];

    const entries = {
      get: vi.fn((id: number) => {
        const d = deferred<EntryFull>();
        getCalls.push({ id, d });
        return d.promise;
      }),
      generateTotp: vi.fn((id: number) => {
        const d = deferred<GeneratedTotp>();
        totpCalls.push({ id, d });
        return d.promise;
      }),
      passwordHistory: vi.fn(async () => []),
      clearPasswordHistory: vi.fn(async () => 0),
      setFavorite: vi.fn(async () => undefined),
      remove: vi.fn(async () => undefined),
    };

    TestBed.configureTestingModule({
      imports: [EntryDetailComponent],
      providers: [
        { provide: ActivatedRoute, useValue: { paramMap: params.asObservable() } },
        { provide: Router, useValue: { navigate: vi.fn(async () => true) } },
        { provide: PasswordEntryService, useValue: entries },
        { provide: ClipboardService, useValue: { copy: vi.fn(async () => undefined) } },
        { provide: CategoryService, useValue: { categories: () => [] } },
        { provide: ConfirmService, useValue: { ask: vi.fn(async () => false) } },
      ],
    });
  });

  /** Let queued promise callbacks run, then re-render. */
  async function settle(fixture: { detectChanges: () => void }) {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();
  }

  function call<T>(list: { id: number; d: ReturnType<typeof deferred<T>> }[], id: number) {
    const found = list.find((c) => c.id === id);
    if (!found) throw new Error(`no pending call for entry ${id}`);
    return found.d;
  }

  it('shows the new entry code when a navigation lands mid-refresh of the old one', async () => {
    // Entry 1's TOTP fetch is still in flight when the user opens entry 2. A
    // single-flight guard that is not per-entry makes entry 2 skip its own
    // fetch entirely, so its code never appears and no countdown ever starts -
    // the entry silently looks like it has no 2FA.
    const fixture = TestBed.createComponent(EntryDetailComponent);
    fixture.detectChanges();

    call(getCalls, 1).resolve(entry(1, { hasTotp: true }));
    await settle(fixture);
    expect(totpCalls.some((c) => c.id === 1)).toBe(true);

    // Navigate to entry 2 while entry 1's code request is still pending.
    params.next(convertToParamMap({ id: '2' }));
    call(getCalls, 2).resolve(entry(2, { hasTotp: true }));
    await settle(fixture);

    // Entry 1's request finally comes back; its result must be dropped.
    call(totpCalls, 1).resolve({ code: '111111', secondsRemaining: 20, period: 30 });
    await settle(fixture);

    expect(
      totpCalls.some((c) => c.id === 2),
      'entry 2 never asked for its own code',
    ).toBe(true);

    call(totpCalls, 2).resolve({ code: '222222', secondsRemaining: 25, period: 30 });
    await settle(fixture);

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('222 222');
    expect(text).not.toContain('111 111');

    fixture.destroy();
  });

  it('drops a stale entry load rather than showing it over a newer one', async () => {
    const fixture = TestBed.createComponent(EntryDetailComponent);
    fixture.detectChanges();

    params.next(convertToParamMap({ id: '2' }));
    // Entry 2 arrives first, then entry 1's older request finally lands.
    call(getCalls, 2).resolve(entry(2));
    await settle(fixture);
    call(getCalls, 1).resolve(entry(1));
    await settle(fixture);

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Entry 2');
    expect(text).not.toContain('Entry 1');

    fixture.destroy();
  });

  it('does not attach the previous entry history to the entry now on screen', async () => {
    const fixture = TestBed.createComponent(EntryDetailComponent);
    fixture.detectChanges();
    call(getCalls, 1).resolve(entry(1));
    await settle(fixture);

    const entries = TestBed.inject(PasswordEntryService) as unknown as {
      passwordHistory: ReturnType<typeof vi.fn>;
    };
    const pending = deferred<{ id: number; password: string; changedAt: string }[]>();
    entries.passwordHistory.mockReturnValueOnce(pending.promise);

    const component = fixture.componentInstance as unknown as {
      loadHistory: () => Promise<void>;
      history: () => unknown[] | null;
    };
    void component.loadHistory();
    await settle(fixture);

    params.next(convertToParamMap({ id: '2' }));
    call(getCalls, 2).resolve(entry(2));
    await settle(fixture);

    pending.resolve([{ id: 9, password: 'old-secret', changedAt: '2026-01-01T00:00:00Z' }]);
    await settle(fixture);

    expect(component.history()).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('old-secret');

    fixture.destroy();
  });

  it('masks a secret custom field until it is revealed, and re-masks on navigation', async () => {
    const fixture = TestBed.createComponent(EntryDetailComponent);
    fixture.detectChanges();
    call(getCalls, 1).resolve(
      entry(1, {
        fields: [
          { label: 'Recovery code', value: 'abc-123', secret: true },
          { label: 'Support PIN', value: '4242', secret: false },
        ],
      }),
    );
    await settle(fixture);

    let text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Recovery code');
    expect(text).not.toContain('abc-123');
    // A field the user marked non-secret is readable straight away.
    expect(text).toContain('4242');

    const component = fixture.componentInstance as unknown as {
      toggleFieldReveal: (i: number) => void;
      revealedFieldIndex: () => number | null;
    };
    component.toggleFieldReveal(0);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('abc-123');

    // Opening another entry must not carry the revealed state across.
    params.next(convertToParamMap({ id: '2' }));
    call(getCalls, 2).resolve(
      entry(2, { fields: [{ label: 'Recovery code', value: 'zzz-999', secret: true }] }),
    );
    await settle(fixture);
    text = fixture.nativeElement.textContent as string;
    expect(component.revealedFieldIndex()).toBeNull();
    expect(text).not.toContain('zzz-999');

    fixture.destroy();
  });

  it('reports an invalid route id instead of requesting it', async () => {
    params.next(convertToParamMap({ id: 'not-a-number' }));
    const fixture = TestBed.createComponent(EntryDetailComponent);
    fixture.detectChanges();
    await settle(fixture);

    expect(getCalls).toHaveLength(0);
    expect(fixture.nativeElement.textContent).toContain('Invalid entry id.');

    fixture.destroy();
  });
});
