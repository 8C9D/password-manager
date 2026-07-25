import { Component, computed, input } from '@angular/core';

import { scorePassword, strengthForBits } from '../../core/services/password-strength';

@Component({
  selector: 'app-password-strength-meter',
  standalone: true,
  templateUrl: './password-strength-meter.component.html',
  styleUrl: './password-strength-meter.component.css',
})
export class PasswordStrengthMeterComponent {
  readonly password = input.required<string>();
  /**
   * Exact entropy, when the caller knows it. The character-based estimate reads
   * a generated passphrase as a long lowercase string and misjudges it, so a
   * caller that counted its own random choices should pass them here instead.
   */
  readonly entropyBits = input<number | null>(null);

  protected readonly strength = computed(() => {
    const bits = this.entropyBits();
    return bits === null ? scorePassword(this.password()) : strengthForBits(bits);
  });
}
