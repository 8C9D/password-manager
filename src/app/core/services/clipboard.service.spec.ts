import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The Angular unit-test system refuses vi.mock on relative paths, so the seam
// is the Tauri package itself - `call` is a thin wrapper over `invoke`.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';

import { ClipboardService } from './clipboard.service';

const mockedCall = vi.mocked(invoke);

describe('ClipboardService', () => {
  let service: ClipboardService;

  beforeEach(() => {
    vi.useFakeTimers();
    mockedCall.mockReset();
    service = new ClipboardService();
  });

  afterEach(() => {
    service.reset();
    vi.useRealTimers();
  });

  it('shows the banner and counts down after a copy', async () => {
    mockedCall.mockResolvedValue(15);
    await service.copy('s3cret', 'Password');
    expect(service.lastCopiedLabel()).toBe('Password');
    expect(service.remainingSecs()).toBe(15);

    vi.advanceTimersByTime(1000);
    expect(service.remainingSecs()).toBe(14);
  });

  it('drops a copy that resolves after the vault locked', async () => {
    // The backend wipes the OS clipboard on lock, so a banner that reappears
    // afterwards both lies about the clipboard and leaks that a secret was
    // copied in the session that just ended.
    let resolveCopy!: (secs: number) => void;
    mockedCall.mockReturnValue(
      new Promise<number>((resolve) => {
        resolveCopy = resolve;
      }),
    );

    const pending = service.copy('s3cret', 'Password');
    service.reset();
    resolveCopy(15);
    await pending;

    expect(service.lastCopiedLabel()).toBeNull();
    expect(service.remainingSecs()).toBe(0);

    // And no countdown may be left ticking behind the lock screen.
    vi.advanceTimersByTime(5000);
    expect(service.lastCopiedLabel()).toBeNull();
  });

  it('lets a newer copy win over a slower earlier one', async () => {
    let resolveFirst!: (secs: number) => void;
    mockedCall.mockReturnValueOnce(
      new Promise<number>((resolve) => {
        resolveFirst = resolve;
      }),
    );
    const first = service.copy('one', 'Username');

    mockedCall.mockResolvedValueOnce(20);
    await service.copy('two', 'Password');

    resolveFirst(15);
    await first;

    expect(service.lastCopiedLabel()).toBe('Password');
    expect(service.remainingSecs()).toBe(20);
  });
});
