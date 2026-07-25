import { Component, inject, OnInit, signal } from '@angular/core';

import { DeletedEntry } from '../../core/models/entry.model';
import { ConfirmService } from '../../core/services/confirm.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-vault-trash',
  standalone: true,
  templateUrl: './vault-trash.component.html',
  styleUrl: './vault-trash.component.css',
})
export class VaultTrashComponent implements OnInit {
  private readonly entries = inject(PasswordEntryService);
  private readonly confirmSvc = inject(ConfirmService);

  protected readonly items = signal<DeletedEntry[]>([]);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);
  // Which row is mid-action, so only that row's buttons go quiet.
  protected readonly busyId = signal<number | null>(null);
  protected readonly emptying = signal(false);

  async ngOnInit(): Promise<void> {
    await this.load();
  }

  private async load(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      this.items.set(await this.entries.listDeleted());
    } catch (e) {
      this.error.set(formatBackendError(e));
    } finally {
      this.loading.set(false);
    }
  }

  protected formatDate(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }

  protected async onRestore(item: DeletedEntry): Promise<void> {
    if (this.busyId() !== null) return;
    this.busyId.set(item.id);
    this.error.set(null);
    try {
      await this.entries.restore(item.id);
      await this.load();
    } catch (e) {
      this.error.set(formatBackendError(e));
    } finally {
      this.busyId.set(null);
    }
  }

  protected async onPurge(item: DeletedEntry): Promise<void> {
    if (this.busyId() !== null) return;
    const ok = await this.confirmSvc.ask({
      title: 'Delete permanently?',
      message: `"${item.title}" and every previous password kept for it will be destroyed. This cannot be undone.`,
      confirmLabel: 'Delete forever',
      danger: true,
    });
    if (!ok) return;
    this.busyId.set(item.id);
    this.error.set(null);
    try {
      await this.entries.purge(item.id);
      await this.load();
    } catch (e) {
      this.error.set(formatBackendError(e));
    } finally {
      this.busyId.set(null);
    }
  }

  protected async onEmpty(): Promise<void> {
    const count = this.items().length;
    if (count === 0 || this.emptying()) return;
    const ok = await this.confirmSvc.ask({
      title: 'Empty the trash?',
      message: `All ${count} ${count === 1 ? 'entry' : 'entries'} in the trash, and every previous password kept for them, will be destroyed. This cannot be undone.`,
      confirmLabel: 'Empty trash',
      danger: true,
    });
    if (!ok) return;
    this.emptying.set(true);
    this.error.set(null);
    try {
      await this.entries.purgeAll();
      await this.load();
    } catch (e) {
      this.error.set(formatBackendError(e));
    } finally {
      this.emptying.set(false);
    }
  }
}
