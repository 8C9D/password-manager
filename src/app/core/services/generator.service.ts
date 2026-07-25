import { Injectable } from '@angular/core';

import {
  GeneratedPassphrase,
  GeneratorOptions,
  PassphraseOptions,
} from '../models/generator.model';
import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class GeneratorService {
  async generate(options: GeneratorOptions): Promise<string> {
    return call<string>('generate_password', { options });
  }

  async generatePassphrase(options: PassphraseOptions): Promise<GeneratedPassphrase> {
    return call<GeneratedPassphrase>('generate_passphrase', { options });
  }
}
