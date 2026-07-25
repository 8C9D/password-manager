import { Component, computed, effect, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink, RouterLinkActive } from '@angular/router';

import { EntrySummary } from '../../core/models/entry.model';
import { CategoryService } from '../../core/services/category.service';
import { ConfirmService } from '../../core/services/confirm.service';
import {
  EntrySortMode,
  filterEntries,
  PasswordEntryService,
  sortEntries,
} from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

/**
 * How a completed bulk action should read.
 *
 * The count comes from the backend, not from the selection, because ids that
 * name nothing live are not changed. Saying "moved 5" when 3 moved would be a
 * lie about what happened to the vault.
 */
export function describeBulkResult(
  verb: string,
  changed: number,
  selected: number,
): string {
  const noun = changed === 1 ? 'entry' : 'entries';
  if (changed === selected) return `${verb} ${changed} ${noun}.`;
  return `${verb} ${changed} of ${selected} selected ${
    selected === 1 ? 'entry' : 'entries'
  }; the rest were no longer there.`;
}

/**
 * Keep a selection to the entries still on screen.
 *
 * Filtering or searching hides entries, and acting on something the user can no
 * longer see is exactly the kind of surprise a bulk action must not spring.
 */
export function retainVisible(
  selected: ReadonlySet<number>,
  visible: readonly EntrySummary[],
): Set<number> {
  const onScreen = new Set(visible.map((e) => e.id));
  return new Set([...selected].filter((id) => onScreen.has(id)));
}

@Component({
  selector: 'app-entry-list',
  standalone: true,
  imports: [FormsModule, RouterLink, RouterLinkActive],
  templateUrl: './entry-list.component.html',
  styleUrl: './entry-list.component.css',
})
export class EntryListComponent implements OnInit {
  protected readonly entries = inject(PasswordEntryService);
  protected readonly categories = inject(CategoryService);
  private readonly confirmSvc = inject(ConfirmService);

  protected readonly loading = signal(true);
  protected readonly errorMsg = signal<string | null>(null);
  protected readonly sortMode = signal<EntrySortMode>('title');

  protected readonly selecting = signal(false);
  protected readonly selected = signal<ReadonlySet<number>>(new Set());
  protected readonly bulkBusy = signal(false);
  protected readonly bulkNotice = signal<string | null>(null);
  protected moveTarget: number | null = null;

  protected readonly visible = computed(() =>
    sortEntries(
      filterEntries(
        this.entries.entries(),
        this.categories.selected(),
        this.entries.searchQuery(),
      ),
      this.sortMode(),
    ),
  );

  protected readonly selectedIds = computed(() => [...this.selected()]);

  constructor() {
    // Prune the selection as the visible set changes, rather than only masking
    // it at action time. Merely hiding a selected entry behind a search would
    // let it come back the moment the search was cleared - so a selection the
    // user believed they had narrowed would silently widen again, and the next
    // "move to trash" would take entries they never meant to pick.
    effect(() => {
      const shown = this.visible();
      const kept = retainVisible(this.selected(), shown);
      if (kept.size !== this.selected().size) this.selected.set(kept);
      // Nothing left to select: leave the mode rather than sit in an invisible
      // selection state with no control on screen to exit it.
      if (this.selecting() && shown.length === 0) this.selecting.set(false);
    });
  }

  protected readonly allSelected = computed(() => {
    const shown = this.visible();
    return shown.length > 0 && this.selectedIds().length === shown.length;
  });

  async ngOnInit(): Promise<void> {
    try {
      await this.entries.list();
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.loading.set(false);
    }
  }

  protected toggleSelecting(): void {
    const next = !this.selecting();
    this.selecting.set(next);
    if (!next) this.clearSelection();
  }

  protected isSelected(id: number): boolean {
    return this.selected().has(id);
  }

  protected toggle(id: number): void {
    const next = new Set(this.selected());
    if (!next.delete(id)) next.add(id);
    this.selected.set(next);
    this.bulkNotice.set(null);
  }

  protected toggleAll(): void {
    this.selected.set(
      this.allSelected() ? new Set() : new Set(this.visible().map((e) => e.id)),
    );
    this.bulkNotice.set(null);
  }

  private clearSelection(): void {
    this.selected.set(new Set());
    this.moveTarget = null;
    this.bulkNotice.set(null);
  }

  private async runBulk(
    verb: string,
    action: (ids: number[]) => Promise<number>,
  ): Promise<void> {
    const ids = this.selectedIds();
    if (ids.length === 0 || this.bulkBusy()) return;
    this.bulkBusy.set(true);
    this.errorMsg.set(null);
    try {
      const changed = await action(ids);
      this.bulkNotice.set(describeBulkResult(verb, changed, ids.length));
      this.selected.set(new Set());
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.bulkBusy.set(false);
    }
  }

  protected async moveSelected(): Promise<void> {
    const target = this.moveTarget;
    await this.runBulk('Moved', (ids) => this.entries.setEntriesCategory(ids, target));
  }

  protected async favoriteSelected(favorite: boolean): Promise<void> {
    await this.runBulk(favorite ? 'Starred' : 'Unstarred', (ids) =>
      this.entries.setEntriesFavorite(ids, favorite),
    );
  }

  protected async trashSelected(): Promise<void> {
    const ids = this.selectedIds();
    if (ids.length === 0) return;
    const noun = ids.length === 1 ? 'entry' : 'entries';
    const ok = await this.confirmSvc.ask({
      title: `Move ${ids.length} ${noun} to trash?`,
      message: `${ids.length} ${noun} will be moved to the trash. You can restore them from there, or delete them permanently.`,
      confirmLabel: 'Move to trash',
      danger: true,
    });
    if (!ok) return;
    await this.runBulk('Trashed', (selectedIds) =>
      this.entries.removeEntries(selectedIds),
    );
  }
}
