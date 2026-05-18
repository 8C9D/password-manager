import { Component, inject, signal } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';

import { EntryFull } from '../../core/models/entry.model';
import { CategoryService } from '../../core/services/category.service';
import { ClipboardService } from '../../core/services/clipboard.service';
import { ConfirmService } from '../../core/services/confirm.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-entry-detail',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './entry-detail.component.html',
  styleUrl: './entry-detail.component.css',
})
export class EntryDetailComponent {
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

  private async load(id: number): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    this.entry.set(null);
    this.showPassword.set(false);
    try {
      const e = await this.entries.get(id);
      this.entry.set(e);
    } catch (err) {
      this.error.set(formatBackendError(err));
    } finally {
      this.loading.set(false);
    }
  }

  protected toggleShow(): void {
    this.showPassword.update((v) => !v);
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
