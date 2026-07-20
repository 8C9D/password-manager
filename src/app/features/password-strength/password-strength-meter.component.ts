import { Component, computed, input } from '@angular/core';

import { scorePassword } from '../../core/services/password-strength';

@Component({
  selector: 'app-password-strength-meter',
  standalone: true,
  templateUrl: './password-strength-meter.component.html',
  styleUrl: './password-strength-meter.component.css',
})
export class PasswordStrengthMeterComponent {
  readonly password = input.required<string>();

  protected readonly strength = computed(() => scorePassword(this.password()));
}
