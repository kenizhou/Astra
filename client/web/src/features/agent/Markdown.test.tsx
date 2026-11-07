// SPDX-License-Identifier: GPL-3.0-only
// The assistant markdown renderer's pins: the markdown subset renders to
// real elements, raw HTML never parses (React's text escaping is the
// sanitiser; there is no HTML string and no innerHTML anywhere), and links
// ride only the composer's own scheme allowlist with unsafe schemes left
// as the literal text.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const routes = vi.hoisted(() => ({ openUrl: vi.fn(), askMailtoOpen: vi.fn() }));

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: routes.openUrl }));
vi.mock('@/features/composer/mailtoOpen', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askMailtoOpen: routes.askMailtoOpen,
}));

import { Markdown } from './Markdown';

beforeEach(() => {
  routes.openUrl.mockClear();
  routes.askMailtoOpen.mockClear();
});

afterEach(cleanup);

describe('the markdown subset', () => {
  it('renders paragraphs, emphasis, inline code, and lists', () => {
    const { container } = render(
      <Markdown text={'Here is **bold** and *quiet* and `code`.\n\n- one\n- two'} />,
    );

    const bold = screen.getByText('bold');
    expect(bold.tagName).toBe('STRONG');
    const em = screen.getByText('quiet');
    expect(em.tagName).toBe('EM');
    const code = screen.getByText('code');
    expect(code.tagName).toBe('CODE');
    expect(container.querySelectorAll('p').length).toBe(1);
    const items = container.querySelectorAll('li');
    expect(items.length).toBe(2);
    expect(items[0]?.textContent).toBe('one');
  });

  it('renders a heading and a fenced code block', () => {
    const { container } = render(<Markdown text={'## Plan\n\n```\nplain block\n```'} />);

    const heading = screen.getByText('Plan');
    expect(heading.tagName).toBe('H3');
    expect(container.querySelector('pre')?.textContent).toContain('plain block');
  });

  it('renders a safe link as an anchor whose click routes to the OS, never a navigation', () => {
    render(<Markdown text={'See [the docs](https://example.com/a) for more.'} />);

    const link = screen.getByRole('link', { name: 'the docs' });
    expect(link.getAttribute('href')).toBe('https://example.com/a');
    fireEvent.click(link);
    expect(routes.openUrl).toHaveBeenCalledWith('https://example.com/a');
  });

  it('routes a mailto link through the composer ask, never the OS', () => {
    render(<Markdown text={'[write](mailto:boss@example.com) to the boss.'} />);

    const link = screen.getByRole('link', { name: 'write' });
    expect(link.getAttribute('href')).toBe('mailto:boss@example.com');
    fireEvent.click(link);
    expect(routes.askMailtoOpen).toHaveBeenCalledWith('mailto:boss@example.com');
    expect(routes.openUrl).not.toHaveBeenCalled();
  });
});

describe('the sandbox', () => {
  it('renders raw HTML and script as literal text, never elements', () => {
    const { container } = render(
      <Markdown
        text={
          'Look <script>window.pwned = true;</script> and <img src="x" onerror="window.pwned = true"> here.'
        }
      />,
    );

    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    expect((window as unknown as { pwned?: boolean }).pwned).toBeUndefined();
    expect(screen.getByText(/<script>window.pwned = true;<\/script>/)).toBeTruthy();
  });

  it('leaves a javascript link as literal text, no anchor', () => {
    const { container } = render(<Markdown text={'[click me](javascript:alert(1)) now.'} />);

    expect(container.querySelector('a')).toBeNull();
    expect(container.textContent).toContain('[click me](javascript:alert(1))');
  });

  it('leaves an image markdown as literal text, no remote load', () => {
    const { container } = render(<Markdown text={'![pic](https://example.com/x.png)'} />);

    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('a')).toBeNull();
    expect(container.textContent).toContain('![pic](https://example.com/x.png)');
  });
});
