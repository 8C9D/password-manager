import { Component, inject, OnInit, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import {
  DEFAULT_GENERATOR_OPTIONS,
  GeneratorOptions,
} from '../../core/models/generator.model';
import { GeneratorService } from '../../core/services/generator.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-password-generator-panel',
  standalone: true,
  imports: [FormsModule],
  template: `
    <div class="panel">
      <div class="row out">
        <code class="generated">{{ current() || '…' }}</code>
        <button type="button" class="btn small" (click)="regenerate()" [disabled]="busy()">
          Regenerate
        </button>
      </div>
      @if (error()) {
        <p class="warn">{{ error() }}</p>
      }
      <div class="row">
        <label class="length">
          Length: <strong>{{ opts.length }}</strong>
          <input
            type="range"
            min="4"
            max="64"
            [(ngModel)]="opts.length"
            name="length"
            (input)="regenerate()"
          />
        </label>
      </div>
      <div class="row classes">
        <label>
          <input type="checkbox" [(ngModel)]="opts.includeLowercase" name="lc" (change)="regenerate()" />
          a–z
        </label>
        <label>
          <input type="checkbox" [(ngModel)]="opts.includeUppercase" name="uc" (change)="regenerate()" />
          A–Z
        </label>
        <label>
          <input type="checkbox" [(ngModel)]="opts.includeNumbers" name="num" (change)="regenerate()" />
          0–9
        </label>
        <label>
          <input type="checkbox" [(ngModel)]="opts.includeSymbols" name="sym" (change)="regenerate()" />
          Symbols
        </label>
        <label>
          <input type="checkbox" [(ngModel)]="opts.excludeAmbiguous" name="amb" (change)="regenerate()" />
          Exclude ambiguous
        </label>
      </div>
      <div class="row actions">
        <button type="button" class="btn small primary" (click)="use()" [disabled]="!current()">
          Use this password
        </button>
      </div>
    </div>
  `,
  styles: [
    `
      .panel {
        display: flex;
        flex-direction: column;
        gap: 0.55rem;
        padding: 0.85rem;
        margin-top: 0.5rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--surface);
      }
      .row {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        flex-wrap: wrap;
      }
      .out {
        justify-content: space-between;
      }
      .generated {
        flex: 1;
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, monospace;
        font-size: 0.95rem;
        padding: 0.5rem 0.7rem;
        background: var(--bg);
        border: 1px solid var(--border);
        border-radius: 5px;
        word-break: break-all;
      }
      .length {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        width: 100%;
        font-size: 0.85rem;
        color: var(--muted);
      }
      .length input {
        flex: 1;
      }
      .classes {
        gap: 0.85rem;
        font-size: 0.85rem;
        color: var(--text);
      }
      .classes label {
        display: inline-flex;
        align-items: center;
        gap: 0.3rem;
      }
      .actions {
        justify-content: flex-end;
      }
      .warn {
        color: var(--danger);
        margin: 0;
        font-size: 0.85rem;
      }
    `,
  ],
})
export class PasswordGeneratorPanelComponent implements OnInit {
  private readonly generator = inject(GeneratorService);

  readonly accept = output<string>();

  protected opts: GeneratorOptions = { ...DEFAULT_GENERATOR_OPTIONS };
  protected readonly current = signal('');
  protected readonly error = signal<string | null>(null);
  protected readonly busy = signal(false);

  private regenScheduled = false;

  ngOnInit(): void {
    void this.regenerate();
  }

  async regenerate(): Promise<void> {
    if (this.regenScheduled) return;
    this.regenScheduled = true;
    queueMicrotask(async () => {
      this.regenScheduled = false;
      this.busy.set(true);
      this.error.set(null);
      try {
        const pw = await this.generator.generate(this.opts);
        this.current.set(pw);
      } catch (e) {
        this.current.set('');
        this.error.set(formatBackendError(e));
      } finally {
        this.busy.set(false);
      }
    });
  }

  use(): void {
    if (this.current()) this.accept.emit(this.current());
  }
}
