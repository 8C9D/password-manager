import { Component, inject, OnDestroy, signal } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { openUrl } from '@tauri-apps/plugin-opener';

import { EntryFull, PasswordHistoryItem } from '../../core/models/entry.model';
import { CategoryService } from '../../core/services/category.service';
import { ClipboardService } from '../../core/services/clipboard.service';
import { ConfirmService } from '../../core/services/confirm.service';
import {
  formatTotpCode,
  parseHttpUrl,
  PasswordEntryService,
} from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

const REVEAL_HIDE_AFTER_MS = 30_000;

@Component({
  selector: 'app-entry-detail',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './entry-detail.component.html',
  styleUrl: './entry-detail.component.css',
})
export class EntryDetailComponent implements OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly entries = inject(PasswordEntryService);
  private readonly clipboard = inject(ClipboardService);
  private readonly categoriesSvc = inject(CategoryService);
  private readonly confirmSvc = inject(ConfirmService);

  protected readonly entry = signal<EntryFull | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);
  protected readonly showPassword = signal(false);

  protected readonly totpCode = signal<string | null>(null);
  protected readonly totpRemaining = signal(0);
  protected readonly totpPeriod = signal(30);

  // Previous passwords are fetched only when the user asks for them, so simply
  // viewing an entry never pulls its retired secrets into the renderer.
  protected readonly history = signal<PasswordHistoryItem[] | null>(null);
  protected readonly historyBusy = signal(false);
  protected readonly historyError = signal<string | null>(null);
  // At most one retired password is legible at a time.
  protected readonly revealedHistoryId = signal<number | null>(null);

  private hideTimer: ReturnType<typeof setTimeout> | null = null;
  private historyHideTimer: ReturnType<typeof setTimeout> | null = null;
  private totpTimer: ReturnType<typeof setInterval> | null = null;
  private totpEntryId: number | null = null;
  private totpRefreshing = false;
  // Bumped on every load() so a slower in-flight load can detect it was
  // superseded by a newer navigation and skip mutating state.
  private loadToken = 0;

  constructor() {
    this.route.paramMap.pipe(takeUntilDestroyed()).subscribe((params) => {
      const raw = params.get('id');
      const id = raw === null ? NaN : Number(raw);
      if (!Number.isFinite(id) || id <= 0) {
        this.error.set('Invalid entry id.');
        this.loading.set(false);
        return;
      }
      this.load(id);
    });
  }

  protected categoryName(): string | null {
    const e = this.entry();
    if (!e || e.categoryId === null) return null;
    const cat = this.categoriesSvc.categories().find((c) => c.id === e.categoryId);
    return cat?.name ?? null;
  }

  ngOnDestroy(): void {
    this.cancelAutoHide();
    this.cancelHistoryAutoHide();
    this.stopTotp();
  }

  private cancelAutoHide(): void {
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
  }

  private cancelHistoryAutoHide(): void {
    if (this.historyHideTimer !== null) {
      clearTimeout(this.historyHideTimer);
      this.historyHideTimer = null;
    }
  }

  private async load(id: number): Promise<void> {
    const token = ++this.loadToken;
    this.loading.set(true);
    this.error.set(null);
    this.entry.set(null);
    this.showPassword.set(false);
    this.cancelAutoHide();
    this.stopTotp();
    this.totpCode.set(null);
    this.resetHistory();
    try {
      const e = await this.entries.get(id);
      // A newer navigation started while this get() was in flight; drop the
      // stale result so we don't display it or start a leaked TOTP timer.
      if (token !== this.loadToken) return;
      this.entry.set(e);
      if (e.hasTotp) {
        void this.startTotp(e.id);
      }
    } catch (err) {
      if (token !== this.loadToken) return;
      this.error.set(formatBackendError(err));
    } finally {
      if (token === this.loadToken) this.loading.set(false);
    }
  }

  protected formatTotp(code: string): string {
    return formatTotpCode(code);
  }

  private async startTotp(id: number): Promise<void> {
    this.totpEntryId = id;
    await this.refreshTotp();
    // Only start ticking if this is still the active entry and the first fetch
    // succeeded.
    if (this.totpEntryId === id && this.totpCode() !== null && this.totpTimer === null) {
      this.totpTimer = setInterval(() => this.tickTotp(), 1000);
    }
  }

  private async refreshTotp(): Promise<void> {
    // Single-flight: the 1s tick keeps firing while remaining <= 0, and a
    // second concurrent refresh could interleave with this one's completion.
    const id = this.totpEntryId;
    if (id === null || this.totpRefreshing) return;
    this.totpRefreshing = true;
    try {
      const t = await this.entries.generateTotp(id);
      // The user navigated to another entry while this refresh was in
      // flight; dropping the result keeps A's code off B's page.
      if (this.totpEntryId !== id) return;
      this.totpCode.set(t.code);
      this.totpRemaining.set(t.secondsRemaining);
      this.totpPeriod.set(t.period);
    } catch {
      if (this.totpEntryId !== id) return;
      // Leave the rest of the entry usable; just drop the code display.
      this.stopTotp();
      this.totpCode.set(null);
    } finally {
      this.totpRefreshing = false;
    }
  }

  private tickTotp(): void {
    const remaining = this.totpRemaining() - 1;
    if (remaining <= 0) {
      void this.refreshTotp();
    } else {
      this.totpRemaining.set(remaining);
    }
  }

  private stopTotp(): void {
    if (this.totpTimer !== null) {
      clearInterval(this.totpTimer);
      this.totpTimer = null;
    }
    this.totpEntryId = null;
  }

  protected toggleShow(): void {
    this.cancelAutoHide();
    this.showPassword.update((v) => !v);
    // A revealed password re-masks itself after a while.
    if (this.showPassword()) {
      this.hideTimer = setTimeout(() => {
        this.showPassword.set(false);
        this.hideTimer = null;
      }, REVEAL_HIDE_AFTER_MS);
    }
  }

  protected openableUrl(): string | null {
    const e = this.entry();
    return e ? parseHttpUrl(e.urlOrAppName) : null;
  }

  protected async onOpenUrl(): Promise<void> {
    const url = this.openableUrl();
    if (!url) return;
    try {
      await openUrl(url);
    } catch (err) {
      this.error.set(formatBackendError(err));
    }
  }

  protected async copy(value: string, label: string): Promise<void> {
    try {
      await this.clipboard.copy(value, label);
    } catch (err) {
      this.error.set(formatBackendError(err));
    }
  }

  protected async toggleFavorite(): Promise<void> {
    const e = this.entry();
    if (!e) return;
    const next = !e.favorite;
    // Optimistic update; revert if the backend rejects it.
    this.entry.set({ ...e, favorite: next });
    try {
      await this.entries.setFavorite(e.id, next);
    } catch (err) {
      // Only revert if the pane still shows this entry; after a navigation
      // the revert would resurrect the previous entry's data.
      if (this.entry()?.id !== e.id) return;
      this.entry.set({ ...e, favorite: e.favorite });
      this.error.set(formatBackendError(err));
    }
  }

  private resetHistory(): void {
    this.cancelHistoryAutoHide();
    this.history.set(null);
    this.historyError.set(null);
    this.historyBusy.set(false);
    this.revealedHistoryId.set(null);
  }

  protected async loadHistory(): Promise<void> {
    const e = this.entry();
    if (!e || this.historyBusy()) return;
    this.historyBusy.set(true);
    this.historyError.set(null);
    try {
      const rows = await this.entries.passwordHistory(e.id);
      // A navigation may have swapped the entry while this was in flight;
      // showing the previous entry's retired passwords here would be worse
      // than showing nothing.
      if (this.entry()?.id !== e.id) return;
      this.history.set(rows);
    } catch (err) {
      if (this.entry()?.id !== e.id) return;
      this.historyError.set(formatBackendError(err));
    } finally {
      if (this.entry()?.id === e.id) this.historyBusy.set(false);
    }
  }

  protected hideHistory(): void {
    this.resetHistory();
  }

  protected toggleHistoryReveal(id: number): void {
    this.cancelHistoryAutoHide();
    const next = this.revealedHistoryId() === id ? null : id;
    this.revealedHistoryId.set(next);
    if (next !== null) {
      this.historyHideTimer = setTimeout(() => {
        this.revealedHistoryId.set(null);
        this.historyHideTimer = null;
      }, REVEAL_HIDE_AFTER_MS);
    }
  }

  protected async onClearHistory(): Promise<void> {
    const e = this.entry();
    if (!e) return;
    const ok = await this.confirmSvc.ask({
      title: 'Clear password history?',
      message: `Every previous password kept for "${e.title}" will be permanently deleted. This cannot be undone.`,
      confirmLabel: 'Clear',
      danger: true,
    });
    if (!ok) return;
    try {
      await this.entries.clearPasswordHistory(e.id);
      if (this.entry()?.id !== e.id) return;
      this.cancelHistoryAutoHide();
      this.revealedHistoryId.set(null);
      this.history.set([]);
    } catch (err) {
      if (this.entry()?.id !== e.id) return;
      this.historyError.set(formatBackendError(err));
    }
  }

  protected mask(value: string): string {
    return '•'.repeat(Math.min(value.length, 24));
  }

  protected formatDate(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }

  protected async onDelete(id: number, title: string): Promise<void> {
    const ok = await this.confirmSvc.ask({
      title: 'Move to trash?',
      message: `"${title}" will be moved to the trash. You can restore it from there, or delete it permanently.`,
      confirmLabel: 'Move to trash',
      danger: true,
    });
    if (!ok) return;
    try {
      await this.entries.remove(id);
      await this.router.navigate(['/vault']);
    } catch (e) {
      this.error.set(formatBackendError(e));
    }
  }
}
