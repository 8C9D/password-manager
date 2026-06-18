import { Component, inject, OnInit } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CategoryService } from '../../core/services/category.service';

@Component({
  selector: 'app-category-sidebar',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './category-sidebar.component.html',
  styleUrl: './category-sidebar.component.css',
})
export class CategorySidebarComponent implements OnInit {
  protected readonly categories = inject(CategoryService);

  async ngOnInit(): Promise<void> {
    await this.categories.list();
  }

  protected select(id: number | null): void {
    this.categories.select(id);
  }
}
