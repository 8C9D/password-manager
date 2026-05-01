import { Injectable } from '@angular/core';

import { GeneratorOptions } from '../models/generator.model';
import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class GeneratorService {
  async generate(options: GeneratorOptions): Promise<string> {
    return call<string>('generate_password', { options });
  }
}
