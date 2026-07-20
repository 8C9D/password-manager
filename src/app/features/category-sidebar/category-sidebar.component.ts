import { Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CategoryService } from '../../core/services/category.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-category-sidebar',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './category-sidebar.component.html',
  styleUrl: './category-sidebar.component.css',
})
export class CategorySidebarComponent implements OnInit {
  protected readonly categories = inject(CategoryService);

  protected readonly errorMsg = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    try {
      await this.categories.list();
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    }
  }

  protected select(id: number | null): void {
    this.categories.select(id);
  }
}
