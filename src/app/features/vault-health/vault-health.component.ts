import { Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { EntryIssue, VaultHealth } from '../../core/models/entry.model';
import {
  describeIssue,
  PasswordEntryService,
} from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-vault-health',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './vault-health.component.html',
  styleUrl: './vault-health.component.css',
})
export class VaultHealthComponent implements OnInit {
  private readonly entries = inject(PasswordEntryService);

  protected readonly health = signal<VaultHealth | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    await this.run();
  }

  protected async run(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      this.health.set(await this.entries.auditVault());
    } catch (e) {
      this.error.set(formatBackendError(e));
    } finally {
      this.loading.set(false);
    }
  }

  protected labels(issue: EntryIssue): string[] {
    return describeIssue(issue);
  }
}
