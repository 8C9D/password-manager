export type StrengthScore = 0 | 1 | 2 | 3 | 4;

export interface PasswordStrength {
  score: StrengthScore;
  label: string;
}

export const STRENGTH_LABELS: Record<StrengthScore, string> = {
  0: 'Very weak',
  1: 'Weak',
  2: 'Fair',
  3: 'Good',
  4: 'Strong',
};

const COMMON_PASSWORDS = new Set([
  'password',
  'password1',
  'passwort',
  'qwerty',
  'qwertyuiop',
  'azerty',
  '123456',
  '1234567',
  '12345678',
  '123456789',
  '1234567890',
  'letmein',
  'welcome',
  'admin',
  'iloveyou',
  'monkey',
  'dragon',
  'abc123',
  'football',
  'baseball',
  'sunshine',
  'princess',
  'trustno1',
  '000000',
  '111111',
]);

const LEET_MAP: Record<string, string> = {
  '@': 'a',
  '4': 'a',
  '3': 'e',
  '5': 's',
  '0': 'o',
  '1': 'i',
  '7': 't',
  '!': 'i',
  $: 's',
};

function poolSize(pw: string): number {
  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/[0-9]/.test(pw)) pool += 10;
  if (/[^a-zA-Z0-9]/.test(pw)) pool += 33;
  return pool;
}

/**
 * Length after collapsing repeated runs ("aaaa" ≈ 2 chars) and sequential
 * runs ("abcd", "9876" ≈ 2 chars), so padding tricks don't inflate the score.
 */
function effectiveLength(pw: string): number {
  let len = 0;
  let i = 0;
  while (i < pw.length) {
    const start = i;
    const code = pw.charCodeAt(i);
    // Repeated run.
    while (i < pw.length && pw.charCodeAt(i) === code) i++;
    if (i - start >= 2) {
      len += 2;
      continue;
    }
    // Ascending or descending run of consecutive char codes.
    let j = start + 1;
    const dir = j < pw.length ? pw.charCodeAt(j) - pw.charCodeAt(start) : 0;
    if (dir === 1 || dir === -1) {
      while (j < pw.length && pw.charCodeAt(j) - pw.charCodeAt(j - 1) === dir) j++;
      if (j - start >= 3) {
        len += 2;
        i = j;
        continue;
      }
    }
    len += 1;
    i = start + 1;
  }
  return len;
}

export function scorePassword(pw: string): PasswordStrength {
  const finish = (score: StrengthScore): PasswordStrength => ({
    score,
    label: STRENGTH_LABELS[score],
  });

  if (pw.length === 0) return finish(0);

  const normalized = pw.toLowerCase();
  const deLeeted = normalized.replace(/[@435017!$]/g, (c) => LEET_MAP[c] ?? c);
  // A common password, possibly leet-substituted or with trivial
  // suffix/prefix noise stripped.
  for (const candidate of [normalized, deLeeted]) {
    if (
      COMMON_PASSWORDS.has(candidate) ||
      COMMON_PASSWORDS.has(candidate.replace(/[^a-z]+$/, '')) ||
      COMMON_PASSWORDS.has(candidate.replace(/^[^a-z]+/, ''))
    ) {
      return finish(0);
    }
  }

  const bits = effectiveLength(pw) * Math.log2(poolSize(pw));

  if (bits < 20) return finish(0);
  if (bits < 36) return finish(1);
  if (bits < 52) return finish(2);
  if (bits < 68) return finish(3);
  return finish(4);
}
