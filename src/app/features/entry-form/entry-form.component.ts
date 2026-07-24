import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { Category } from '../../core/models/category.model';
import { EntryInput } from '../../core/models/entry.model';
import { CategoryService } from '../../core/services/category.service';
import {
  parseTagsInput,
  PasswordEntryService,
  totpActionFrom,
  validateEntryInput,
} from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';
import { PasswordGeneratorPanelComponent } from '../password-generator/password-generator-panel.component';
import { PasswordStrengthMeterComponent } from '../password-strength/password-strength-meter.component';

@Component({
  selector: 'app-entry-form',
  standalone: true,
  imports: [
    FormsModule,
    RouterLink,
    PasswordGeneratorPanelComponent,
    PasswordStrengthMeterComponent,
  ],
  templateUrl: './entry-form.component.html',
  styleUrl: './entry-form.component.css',
})
export class EntryFormComponent implements OnInit {
  private readonly entries = inject(PasswordEntryService);
  private readonly categoriesSvc = inject(CategoryService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  protected title = '';
  protected username = '';
  protected urlOrAppName = '';
  protected password = '';
  protected notes = '';
  protected categoryId: number | null = null;
  protected totpSecret = '';
  protected removeTotp = false;
  protected hasExistingTotp = false;
  protected favorite = false;
  protected tagsInput = '';

  protected readonly busy = signal(false);
  protected readonly loading = signal(true);
  protected readonly errorMsg = signal<string | null>(null);
  protected readonly showPassword = signal(false);
  protected readonly showGenerator = signal(false);
  protected readonly titleTouched = signal(false);
  protected readonly passwordTouched = signal(false);
  protected readonly categories = signal<Category[]>([]);
  protected readonly editingId = signal<number | null>(null);

  protected isEdit = () => this.editingId() !== null;
  protected cancelLink = () => {
    const id = this.editingId();
    return id ? ['/vault', id] : ['/vault'];
  };

  async ngOnInit(): Promise<void> {
    const idParam = this.route.snapshot.paramMap.get('id');
    const id = idParam ? Number(idParam) : null;
    if (id !== null && (!Number.isFinite(id) || id <= 0)) {
      // A garbage edit URL must not fall through to an empty form whose
      // submit would then call update() with a nonsense id.
      this.errorMsg.set('Invalid entry id.');
      this.loading.set(false);
      return;
    }
    this.editingId.set(id);
    try {
      await this.categoriesSvc.list();
      this.categories.set(this.categoriesSvc.categories());
      if (id !== null) {
        const full = await this.entries.get(id);
        this.title = full.title;
        this.username = full.username;
        this.urlOrAppName = full.urlOrAppName;
        this.password = full.password;
        this.notes = full.notes ?? '';
        this.categoryId = full.categoryId;
        this.hasExistingTotp = full.hasTotp;
        this.favorite = full.favorite;
        this.tagsInput = full.tags.join(', ');
      }
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.loading.set(false);
    }
  }

  protected toggleShow(): void {
    this.showPassword.update((v) => !v);
  }

  protected toggleGenerator(): void {
    this.showGenerator.update((v) => !v);
  }

  protected onGenerated(pw: string): void {
    this.password = pw;
    this.passwordTouched.set(true);
    this.showGenerator.set(false);
  }

  protected async onSubmit(): Promise<void> {
    this.titleTouched.set(true);
    this.passwordTouched.set(true);
    const validation = validateEntryInput({ title: this.title, password: this.password });
    if (!validation.valid || this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      const input: EntryInput = {
        categoryId: this.categoryId,
        title: this.title.trim(),
        username: this.username,
        urlOrAppName: this.urlOrAppName,
        password: this.password,
        notes: this.notes === '' ? null : this.notes,
        totp: totpActionFrom(this.totpSecret, this.removeTotp),
        favorite: this.favorite,
        tags: parseTagsInput(this.tagsInput),
      };
      const id = this.editingId();
      if (id !== null) {
        await this.entries.update(id, input);
        await this.router.navigate(['/vault', id]);
      } else {
        const newId = await this.entries.create(input);
        await this.router.navigate(['/vault', newId]);
      }
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.busy.set(false);
    }
  }
}
