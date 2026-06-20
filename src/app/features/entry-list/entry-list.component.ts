import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { RouterLink, RouterLinkActive } from '@angular/router';

import { CategoryService } from '../../core/services/category.service';
import {
  filterEntries,
  PasswordEntryService,
} from '../../core/services/password-entry.service';

@Component({
  selector: 'app-entry-list',
  standalone: true,
  imports: [RouterLink, RouterLinkActive],
  templateUrl: './entry-list.component.html',
  styleUrl: './entry-list.component.css',
})
export class EntryListComponent implements OnInit {
  protected readonly entries = inject(PasswordEntryService);
  protected readonly categories = inject(CategoryService);

  protected readonly loading = signal(true);

  protected readonly visible = computed(() =>
    filterEntries(
      this.entries.entries(),
      this.categories.selected(),
      this.entries.searchQuery(),
    ),
  );

  async ngOnInit(): Promise<void> {
    try {
      await this.entries.list();
    } finally {
      this.loading.set(false);
    }
  }
}
