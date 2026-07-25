import { describe, expect, it } from 'vitest';

import {
  scoreForBits,
  scorePassword,
  strengthForBits,
  STRENGTH_LABELS,
} from './password-strength';

describe('scorePassword', () => {
  it('scores the empty password 0', () => {
    expect(scorePassword('').score).toBe(0);
  });

  it('scores common passwords 0 regardless of length or classes', () => {
    for (const pw of ['password', 'qwerty', '123456789', 'letmein']) {
      expect(scorePassword(pw).score, pw).toBe(0);
    }
  });

  it('catches common passwords in leet disguise or with case changes', () => {
    expect(scorePassword('P@ssw0rd').score).toBe(0);
    expect(scorePassword('QWERTY').score).toBe(0);
  });

  it('catches common passwords with trivial digit suffixes', () => {
    expect(scorePassword('password123').score).toBe(0);
  });

  it('scores single-character repetition very low', () => {
    expect(scorePassword('aaaaaaaaaa').score).toBe(0);
  });

  it('scores sequential runs very low', () => {
    expect(scorePassword('abcdefghij').score).toBeLessThanOrEqual(1);
    expect(scorePassword('9876543210').score).toBeLessThanOrEqual(1);
  });

  it('scores short random-looking passwords low', () => {
    expect(scorePassword('kx3f').score).toBeLessThanOrEqual(1);
  });

  it('scores a medium lowercase password as fair', () => {
    expect(scorePassword('plumbago').score).toBe(2);
  });

  it('scores longer mixed-class passwords higher', () => {
    const fair = scorePassword('grape42fish').score;
    const strong = scorePassword('grape42fish!Kettle9').score;
    expect(strong).toBeGreaterThan(fair);
    expect(strong).toBe(4);
  });

  it('scores long passphrases 4', () => {
    expect(scorePassword('correct horse battery staple').score).toBe(4);
  });

  it('never blocks: always returns a label matching the score', () => {
    for (const pw of ['', 'a', 'password', 'Tr0ub4dor&3']) {
      const r = scorePassword(pw);
      expect(r.label).toBe(STRENGTH_LABELS[r.score]);
    }
  });
});

describe('scoreForBits', () => {
  it('bands bits the same way the character estimate does', () => {
    // The passphrase generator knows its entropy exactly and reuses this
    // banding, so the two paths must never describe the same secret
    // differently.
    expect(scoreForBits(0)).toBe(0);
    expect(scoreForBits(19.9)).toBe(0);
    expect(scoreForBits(20)).toBe(1);
    expect(scoreForBits(35.9)).toBe(1);
    expect(scoreForBits(36)).toBe(2);
    expect(scoreForBits(52)).toBe(3);
    expect(scoreForBits(68)).toBe(4);
    expect(scoreForBits(1000)).toBe(4);
  });

  it('labels bits with the shared label table', () => {
    expect(strengthForBits(60).label).toBe(STRENGTH_LABELS[3]);
    // A 5-word passphrase from a 4096-word list is 60 bits.
    expect(strengthForBits(60).score).toBe(3);
  });
});
