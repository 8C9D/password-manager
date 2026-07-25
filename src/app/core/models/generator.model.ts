export interface GeneratorOptions {
  length: number;
  includeLowercase: boolean;
  includeUppercase: boolean;
  includeNumbers: boolean;
  includeSymbols: boolean;
  excludeAmbiguous: boolean;
}

export const DEFAULT_GENERATOR_OPTIONS: GeneratorOptions = {
  length: 24,
  includeLowercase: true,
  includeUppercase: true,
  includeNumbers: true,
  includeSymbols: true,
  excludeAmbiguous: false,
};

/** Which kind of secret the generator panel produces. */
export type GeneratorMode = 'password' | 'passphrase';

export interface PassphraseOptions {
  wordCount: number;
  separator: string;
  capitalize: boolean;
  includeNumber: boolean;
}

export const DEFAULT_PASSPHRASE_OPTIONS: PassphraseOptions = {
  wordCount: 5,
  separator: '-',
  capitalize: false,
  includeNumber: false,
};

export const MIN_PASSPHRASE_WORDS = 3;
export const MAX_PASSPHRASE_WORDS = 12;

export interface GeneratedPassphrase {
  passphrase: string;
  /**
   * Exact entropy of the choices the backend made. The character-based
   * strength meter reads a passphrase as a long lowercase string and misjudges
   * it, so this number is shown instead.
   */
  entropyBits: number;
}
