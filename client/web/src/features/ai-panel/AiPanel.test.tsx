// SPDX-License-Identifier: GPL-3.0-only
// The AI documents panel: the status toggle opens and shuts it, opening
// pulls the latest digest and report of the panel's account, the tabs
// switch between the two documents, the days selector rides the
// generation ask, the Generate and Regenerate button sends the exact
// payload, and a refused read or generation surfaces the host's sentence
// in the alert region.

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ai = vi.hoisted(() => ({
  digestGenerate: vi.fn(),
  reportGenerate: vi.fn(),
  latestDocument: vi.fn(),
}));

vi.mock('@/services/ai', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  digestGenerate: ai.digestGenerate,
  reportGenerate: ai.reportGenerate,
  latestDocument: ai.latestDocument,
}));

import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useUiStore } from '@/stores/uiStore';

import { useAiPanelStore } from './aiPanelStore';
import { AiPanel, AiStatus } from './AiPanel';

/** One stored digest as the latest read answers it. */
const digestDoc = {
  kind: 'digest',
  period_start: Date.UTC(2026, 9, 3) / 1000,
  period_end: Date.UTC(2026, 9, 9, 23, 59, 59) / 1000,
  content: 'Two replies wait, one newsletter rides.',
  created_at: 3,
};

/** One stored report as the latest read answers it. */
const reportDoc = {
  kind: 'report',
  period_start: Date.UTC(2026, 9, 3) / 1000,
  period_end: Date.UTC(2026, 9, 9, 23, 59, 59) / 1000,
  content: 'A launch week: replies doubled.',
  created_at: 4,
};

/** One generation answer: what the digest and report commands return. */
const generated = {
  period_start: Date.UTC(2026, 10, 2) / 1000,
  period_end: Date.UTC(2026, 10, 8, 23, 59, 59) / 1000,
  content: 'A fresh window.',
  regenerated: false,
};

function reset(): void {
  ai.digestGenerate.mockReset();
  ai.reportGenerate.mockReset();
  ai.latestDocument.mockReset();
  ai.latestDocument.mockResolvedValue(null);
  ai.digestGenerate.mockResolvedValue(generated);
  ai.reportGenerate.mockResolvedValue(generated);
  useAiPanelStore.getState().reset();
  useUiStore.getState().setAiPanelOpen(false);
  useFolderStore.setState({ accounts: [], selected: { account: 'work', key: 'INBOX' } });
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
}

beforeEach(reset);

afterEach(cleanup);

describe('the status toggle', () => {
  it('opens and shuts the panel', () => {
    render(
      <>
        <AiStatus />
        <AiPanel />
      </>,
    );
    expect(screen.queryByRole('region', { name: 'AI documents' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'AI' }));
    });
    expect(screen.getByRole('region', { name: 'AI documents' })).toBeTruthy();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close AI documents' }));
    });
    expect(screen.queryByRole('region', { name: 'AI documents' })).toBeNull();
  });

  it('pulls the latest document of both kinds when it opens', async () => {
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(ai.latestDocument).toHaveBeenCalledTimes(2));
    expect(ai.latestDocument).toHaveBeenCalledWith('work', 'digest');
    expect(ai.latestDocument).toHaveBeenCalledWith('work', 'report');
  });
});

describe('the documents', () => {
  it('renders the latest digest with its period label on the digest tab', async () => {
    ai.latestDocument.mockImplementation(async (_account: string, kind: string) =>
      kind === 'digest' ? digestDoc : null,
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(screen.getByText('Two replies wait, one newsletter rides.')).toBeTruthy());
    expect(screen.getByText('3 Oct 2026 to 9 Oct 2026')).toBeTruthy();
    // The report tab sits unselected, its own empty word waiting.
    expect(screen.getByRole('tab', { name: 'Digest' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('tab', { name: 'Report' }).getAttribute('aria-selected')).toBe('false');
  });

  it('switches to the report tab and renders its document', async () => {
    ai.latestDocument.mockImplementation(async (_account: string, kind: string) =>
      kind === 'report' ? reportDoc : null,
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(screen.getByText('No digest yet.')).toBeTruthy());
    act(() => {
      fireEvent.click(screen.getByRole('tab', { name: 'Report' }));
    });
    expect(screen.getByText('A launch week: replies doubled.')).toBeTruthy();
    // Only the active tab's area renders, so the digest's empty word went
    // with its tab.
    expect(screen.queryByText('No digest yet.')).toBeNull();
  });

  it('shows the loading word while the latest read is in flight', async () => {
    let answer: (value: unknown) => void = () => undefined;
    ai.latestDocument.mockImplementation(
      (_account: string, kind: string) =>
        new Promise((resolve) => {
          if (kind === 'digest') {
            answer = resolve;
          } else {
            resolve(null);
          }
        }),
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    expect(screen.getByText('Loading')).toBeTruthy();
    await act(async () => {
      answer(digestDoc);
      await Promise.resolve();
    });
    await waitFor(() => expect(screen.getByText('Two replies wait, one newsletter rides.')).toBeTruthy());
  });
});

describe('generation', () => {
  it('sends the digest generation with the selected days and renders the answer', async () => {
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    });

    expect(ai.digestGenerate).toHaveBeenCalledWith('work', 7);
    expect(screen.getByText('A fresh window.')).toBeTruthy();
    expect(screen.getByText('2 Nov 2026 to 8 Nov 2026')).toBeTruthy();
  });

  it('rides the days selector on the ask, the full one to thirty-one range', async () => {
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    const selector = screen.getByRole('combobox', { name: 'Window days' }) as HTMLSelectElement;
    expect(selector.value).toBe('7');
    expect(Array.from(selector.options).map((option) => option.value)).toHaveLength(31);
    expect(selector.options[0]?.value).toBe('1');
    expect(selector.options[30]?.value).toBe('31');

    fireEvent.change(selector, { target: { value: '14' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    });
    expect(ai.digestGenerate).toHaveBeenCalledWith('work', 14);
  });

  it('regenerates when a document already shows', async () => {
    ai.latestDocument.mockResolvedValue(digestDoc);
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(screen.getByRole('button', { name: 'Regenerate' })).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Regenerate' }));
    });
    expect(ai.digestGenerate).toHaveBeenCalledWith('work', 7);
  });

  it('generates the report from the report tab under its own command', async () => {
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    act(() => {
      fireEvent.click(screen.getByRole('tab', { name: 'Report' }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    });
    expect(ai.reportGenerate).toHaveBeenCalledWith('work', 7);
    expect(screen.getByText('A fresh window.')).toBeTruthy();
  });

  it('says Generating and refuses a second ask while one is in flight', async () => {
    let answer: (value: unknown) => void = () => undefined;
    ai.digestGenerate.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    const busy = screen.getByRole('button', { name: 'Generating' }) as HTMLButtonElement;
    expect(busy.disabled).toBeTruthy();

    fireEvent.click(busy);
    expect(ai.digestGenerate).toHaveBeenCalledTimes(1);

    await act(async () => {
      answer(generated);
      await Promise.resolve();
    });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Regenerate' })).toBeTruthy());
  });

  it('surfaces a refused generation in the alert region', async () => {
    ai.digestGenerate.mockRejectedValue('the ai surface refused: no bundle runs');
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    });

    const alert = screen.getByRole('alert');
    expect(alert.textContent).toBe('the ai surface refused: no bundle runs');
    expect(useAiPanelStore.getState().docs.digest.error).toEqual({
      source: 'generate',
      message: 'the ai surface refused: no bundle runs',
    });
    // The refusal kept the generation offered and the slot honest: still
    // empty, so the button still says Generate.
    expect(screen.getByText('No digest yet.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Generate' })).toBeTruthy();
  });

  it('keeps the stored document showing under a failed regeneration', async () => {
    ai.latestDocument.mockResolvedValue(digestDoc);
    ai.digestGenerate.mockRejectedValue('the ai surface refused: no bundle runs');
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() =>
      expect(screen.getByText('Two replies wait, one newsletter rides.')).toBeTruthy(),
    );
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Regenerate' }));
    });

    // The refusal paints the alert, and the stored digest stays on show
    // under it: a failed regeneration hides nothing.
    expect(screen.getByRole('alert').textContent).toBe('the ai surface refused: no bundle runs');
    expect(screen.getByText('Two replies wait, one newsletter rides.')).toBeTruthy();
    expect(screen.getByText('3 Oct 2026 to 9 Oct 2026')).toBeTruthy();
  });

  it('surfaces a refused read in the alert region', async () => {
    ai.latestDocument.mockImplementation(
      (_account: string, kind: string) =>
        kind === 'digest' ? Promise.reject('the ai surface refused: the store refused') : Promise.resolve(null),
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('the ai surface refused: the store refused'));
    expect(useAiPanelStore.getState().docs.digest.error).toEqual({
      source: 'read',
      message: 'the ai surface refused: the store refused',
    });
  });
});

describe('the account', () => {
  it('serves the selected folder account', async () => {
    useFolderStore.setState({ selected: { account: 'home', key: 'INBOX' } });
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(ai.latestDocument).toHaveBeenCalledWith('home', 'digest'));
  });

  it('falls back to the first configured account when no folder is selected', async () => {
    useFolderStore.setState({ selected: null });
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(ai.latestDocument).toHaveBeenCalledWith('work', 'digest'));
  });

  it('switches accounts under the selector and re-pulls both kinds', async () => {
    useAccountStore.setState({
      accounts: [
        { accountId: 'work', displayName: 'Work' },
        { accountId: 'home', displayName: 'Home' },
      ],
    });
    ai.latestDocument.mockClear();
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    const selector = screen.getByRole('combobox', { name: 'Document account' }) as HTMLSelectElement;
    expect(selector.value).toBe('work');
    fireEvent.change(selector, { target: { value: 'home' } });

    await waitFor(() => expect(ai.latestDocument).toHaveBeenCalledWith('home', 'report'));
    expect(useAiPanelStore.getState().account).toBe('home');
  });
});

describe('the account switch race', () => {
  /** Seeds both accounts so the selector shows and the panel can switch. */
  function seedTwoAccounts(): void {
    useAccountStore.setState({
      accounts: [
        { accountId: 'work', displayName: 'Work' },
        { accountId: 'home', displayName: 'Home' },
      ],
    });
  }

  /** Switches the panel to the home account through the selector. */
  async function switchToHome(): Promise<void> {
    fireEvent.change(screen.getByRole('combobox', { name: 'Document account' }), {
      target: { value: 'home' },
    });
    await waitFor(() => expect(useAiPanelStore.getState().account).toBe('home'));
  }

  it('drops the old account answer when the account switches mid-generate', async () => {
    seedTwoAccounts();
    let answerWork: (value: unknown) => void = () => undefined;
    ai.digestGenerate.mockImplementation(
      () =>
        new Promise((resolve) => {
          answerWork = resolve;
        }),
    );
    ai.latestDocument.mockResolvedValue(null);
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(useAiPanelStore.getState().account).toBe('work'));
    fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    expect(useAiPanelStore.getState().generating).toBe('digest');

    await switchToHome();
    // The switch owns the button again: nothing of the old account's ask
    // keeps it held.
    expect(useAiPanelStore.getState().generating).toBeNull();
    const button = screen.getByRole('button', { name: 'Generate' }) as HTMLButtonElement;
    expect(button.disabled).toBeFalsy();

    await act(async () => {
      answerWork({ ...generated, content: 'The old work window.' });
      await Promise.resolve();
    });

    // The stale answer never lands in the new account's slot.
    expect(useAiPanelStore.getState().docs.digest.document).toBeNull();
    expect(screen.queryByText('The old work window.')).toBeNull();
    expect(screen.getByText('No digest yet.')).toBeTruthy();
  });

  it('drops the old account refusal when the account switches mid-generate', async () => {
    seedTwoAccounts();
    let refuseWork: () => void = () => undefined;
    ai.digestGenerate.mockImplementation(
      () =>
        new Promise((_resolve, reject) => {
          refuseWork = reject;
        }),
    );
    ai.latestDocument.mockResolvedValue(null);
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(useAiPanelStore.getState().account).toBe('work'));
    fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    await switchToHome();

    await act(async () => {
      refuseWork();
      await Promise.resolve();
    });

    expect(useAiPanelStore.getState().docs.digest.error).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('drops the old account pull when the account switches mid-read', async () => {
    seedTwoAccounts();
    let answerWork: (value: unknown) => void = () => undefined;
    ai.latestDocument.mockImplementation(
      (account: string, kind: string) =>
        new Promise((resolve) => {
          if (account === 'work' && kind === 'digest') {
            answerWork = resolve;
          } else {
            resolve(null);
          }
        }),
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(useAiPanelStore.getState().account).toBe('work'));
    await switchToHome();

    await act(async () => {
      answerWork(digestDoc);
      await Promise.resolve();
    });

    expect(useAiPanelStore.getState().docs.digest.document).toBeNull();
    expect(screen.queryByText('Two replies wait, one newsletter rides.')).toBeNull();
    expect(screen.getByText('No digest yet.')).toBeTruthy();
  });

  it('drops the old account read refusal when the account switches mid-read', async () => {
    seedTwoAccounts();
    let refuseWork: () => void = () => undefined;
    ai.latestDocument.mockImplementation(
      (account: string, kind: string) =>
        new Promise((resolve, reject) => {
          if (account === 'work' && kind === 'digest') {
            refuseWork = reject;
          } else {
            resolve(null);
          }
        }),
    );
    useUiStore.getState().setAiPanelOpen(true);
    render(<AiPanel />);

    await waitFor(() => expect(useAiPanelStore.getState().account).toBe('work'));
    await switchToHome();

    await act(async () => {
      refuseWork();
      await Promise.resolve();
    });

    expect(useAiPanelStore.getState().docs.digest.error).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
