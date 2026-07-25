import { Component, inject, OnInit, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import {
  DEFAULT_GENERATOR_OPTIONS,
  DEFAULT_PASSPHRASE_OPTIONS,
  GeneratorMode,
  GeneratorOptions,
  MAX_PASSPHRASE_WORDS,
  MIN_PASSPHRASE_WORDS,
  PassphraseOptions,
} from '../../core/models/generator.model';
import { ClipboardService } from '../../core/services/clipboard.service';
import { GeneratorService } from '../../core/services/generator.service';
import { formatBackendError } from '../../core/services/tauri-invoke';
import { PasswordStrengthMeterComponent } from '../password-strength/password-strength-meter.component';

@Component({
  selector: 'app-password-generator-panel',
  standalone: true,
  imports: [FormsModule, PasswordStrengthMeterComponent],
  templateUrl: './password-generator-panel.component.html',
  styleUrl: './password-generator-panel.component.css',
})
export class PasswordGeneratorPanelComponent implements OnInit {
  private readonly generator = inject(GeneratorService);
  private readonly clipboard = inject(ClipboardService);

  readonly accept = output<string>();

  protected readonly minWords = MIN_PASSPHRASE_WORDS;
  protected readonly maxWords = MAX_PASSPHRASE_WORDS;

  protected mode: GeneratorMode = 'password';
  protected opts: GeneratorOptions = { ...DEFAULT_GENERATOR_OPTIONS };
  protected phraseOpts: PassphraseOptions = { ...DEFAULT_PASSPHRASE_OPTIONS };

  protected readonly current = signal('');
  /** Exact entropy for a passphrase; null in password mode, where the meter's
   * character-based estimate is the right model. */
  protected readonly entropyBits = signal<number | null>(null);
  protected readonly error = signal<string | null>(null);
  protected readonly busy = signal(false);

  private regenScheduled = false;
  // Bumped per request so an out-of-order response (slider drags fire many
  // overlapping generate calls) can't display a password that doesn't match
  // the currently selected options.
  private regenSeq = 0;

  /** The exact entropy, rounded for display. */
  protected roundedEntropy(): number {
    return Math.round(this.entropyBits() ?? 0);
  }

  ngOnInit(): void {
    void this.regenerate();
  }

  protected setMode(mode: GeneratorMode): void {
    if (this.mode === mode) return;
    this.mode = mode;
    // The old secret belongs to the other mode; drop it rather than leave it
    // on screen under controls that no longer describe it.
    this.current.set('');
    this.entropyBits.set(null);
    void this.regenerate();
  }

  async regenerate(): Promise<void> {
    if (this.regenScheduled) return;
    this.regenScheduled = true;
    queueMicrotask(async () => {
      this.regenScheduled = false;
      const seq = ++this.regenSeq;
      this.busy.set(true);
      this.error.set(null);
      try {
        if (this.mode === 'passphrase') {
          const out = await this.generator.generatePassphrase(this.phraseOpts);
          if (seq !== this.regenSeq) return;
          this.current.set(out.passphrase);
          this.entropyBits.set(out.entropyBits);
        } else {
          const pw = await this.generator.generate(this.opts);
          if (seq !== this.regenSeq) return;
          this.current.set(pw);
          this.entropyBits.set(null);
        }
      } catch (e) {
        if (seq !== this.regenSeq) return;
        this.current.set('');
        this.entropyBits.set(null);
        this.error.set(formatBackendError(e));
      } finally {
        if (seq === this.regenSeq) this.busy.set(false);
      }
    });
  }

  use(): void {
    if (this.current()) this.accept.emit(this.current());
  }

  async copy(): Promise<void> {
    const pw = this.current();
    if (!pw) return;
    try {
      await this.clipboard.copy(pw, this.mode === 'passphrase' ? 'generated passphrase' : 'generated password');
    } catch (e) {
      this.error.set(formatBackendError(e));
    }
  }
}
