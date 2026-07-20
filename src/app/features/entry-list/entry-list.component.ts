import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink, RouterLinkActive } from '@angular/router';

import { CategoryService } from '../../core/services/category.service';
import {
  EntrySortMode,
  filterEntries,
  PasswordEntryService,
  sortEntries,
} from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

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

  protected readonly loading = signal(true);
  protected readonly errorMsg = signal<string | null>(null);
  protected readonly sortMode = signal<EntrySortMode>('title');

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

  async ngOnInit(): Promise<void> {
    try {
      await this.entries.list();
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.loading.set(false);
    }
  }
}
