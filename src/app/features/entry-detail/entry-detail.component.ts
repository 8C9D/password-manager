import { Component, inject, OnDestroy, signal } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { openUrl } from '@tauri-apps/plugin-opener';

import { EntryFull } from '../../core/models/entry.model';
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

  private hideTimer: ReturnType<typeof setTimeout> | null = null;
  private totpTimer: ReturnType<typeof setInterval> | null = null;
  private totpEntryId: number | null = null;

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
    this.stopTotp();
  }

  private cancelAutoHide(): void {
    if (this.hideTimer !== null) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
  }

  private async load(id: number): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    this.entry.set(null);
    this.showPassword.set(false);
    this.cancelAutoHide();
    this.stopTotp();
    this.totpCode.set(null);
    try {
      const e = await this.entries.get(id);
      this.entry.set(e);
      if (e.hasTotp) {
        void this.startTotp(e.id);
      }
    } catch (err) {
      this.error.set(formatBackendError(err));
    } finally {
      this.loading.set(false);
    }
  }

  protected formatTotp(code: string): string {
    return formatTotpCode(code);
  }

  private async startTotp(id: number): Promise<void> {
    this.totpEntryId = id;
    await this.refreshTotp();
    // Only start ticking if the first fetch succeeded.
    if (this.totpCode() !== null && this.totpTimer === null) {
      this.totpTimer = setInterval(() => this.tickTotp(), 1000);
    }
  }

  private async refreshTotp(): Promise<void> {
    if (this.totpEntryId === null) return;
    try {
      const t = await this.entries.generateTotp(this.totpEntryId);
      this.totpCode.set(t.code);
      this.totpRemaining.set(t.secondsRemaining);
      this.totpPeriod.set(t.period);
    } catch {
      // Leave the rest of the entry usable; just drop the code display.
      this.stopTotp();
      this.totpCode.set(null);
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
    await this.clipboard.copy(value, label);
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
      title: 'Delete entry?',
      message: `"${title}" will be permanently deleted. This cannot be undone.`,
      confirmLabel: 'Delete',
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
