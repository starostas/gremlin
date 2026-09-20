import type { ComponentChildren } from 'preact';
import { useEffect, useState } from 'preact/hooks';

/**
 * Chrome shared by every island so the experiments read the same way: one
 * labelled band per purpose, in the same order. Setup is what the search is
 * given, View is anything that moves the picture without re-running a search,
 * Result is what came out, and Export is how to take it away.
 *
 * This lives apart from ExperimentIsland because the Shader Sculptor and Tiny
 * Robot panels are imported by it, and importing back would be a cycle.
 */
export function Section({ title, children }: { title: string; children: ComponentChildren }) {
  return (
    <section class="experiment-section">
      <h3 class="experiment-section-title">{title}</h3>
      {children}
    </section>
  );
}

const defaultMaxLines = 100;

/**
 * A metric with the direction that counts as progress, so a falling line is not
 * read as a failing one. `better` is left unset for counters like a generation
 * number or an evaluation total, where neither direction is an improvement.
 */
export function Metric({
  label,
  value,
  better
}: {
  label: string;
  value: string | number | undefined;
  better?: 'higher' | 'lower';
}) {
  return (
    <div class="experiment-metric">
      <span>
        {label}
        {better && (
          <abbr class="experiment-metric-goal" title={`${better} is better`}>
            {better === 'higher' ? '↑' : '↓'}
          </abbr>
        )}
      </span>
      <strong>{value ?? '—'}</strong>
    </div>
  );
}

/**
 * Discovered programs vary from a single very long line to several thousand
 * short ones, so both extremes are handled here rather than at each call site:
 * long lines wrap instead of scrolling off, and a long program is cut to a
 * readable extract that says how much was left out.
 */
export function CodeBlock({ text, maxLines = defaultMaxLines }: { text: string; maxLines?: number }) {
  const lines = text.replace(/\s+$/, '').split('\n');
  const remaining = lines.length - maxLines;
  const shown = remaining > 0 ? lines.slice(0, maxLines).join('\n') : lines.join('\n');
  return (
    <>
      <pre class="experiment-code-block"><code>{shown}</code></pre>
      {remaining > 0 && (
        <p class="experiment-note">
          [… {new Intl.NumberFormat('en-US').format(remaining)} more line{remaining === 1 ? '' : 's'}. Download the program to read it in full.]
        </p>
      )}
    </>
  );
}

/**
 * The same two-panel summary on every island: what the search had to match on
 * the left, what it produced on the right. Where a reference program genuinely
 * exists it is shown as source; where the target is an image, a simulator or an
 * equation the left panel states the objective instead, so nothing is presented
 * as code that is not.
 */
export function Outcome({
  targetLabel,
  target,
  resultLabel,
  result,
  note
}: {
  targetLabel: string;
  target: ComponentChildren;
  resultLabel: string;
  result?: ComponentChildren;
  note?: string;
}) {
  if (!target && !result) return null;
  return (
    <Section title="Result">
      <div class="experiment-code-pair">
        <figure>
          <figcaption>{targetLabel}</figcaption>
          {target}
        </figure>
        <figure>
          <figcaption>{resultLabel}</figcaption>
          {result ?? (
            <p class="experiment-objective">
              No program yet. Replay the recorded run, or start a new one.
            </p>
          )}
        </figure>
      </div>
      {note && <p class="experiment-note">{note}</p>}
    </Section>
  );
}

/**
 * Canvas colours taken from the site's own palette rather than written into the
 * drawing code, so a chart is not a black rectangle on a white page. Starlight
 * inverts these tokens between themes: `--sl-color-black` is the page ground
 * and `--sl-color-white` the ink, whichever theme is active.
 */
export type CanvasPalette = {
  ground: string;
  grid: string;
  ink: string;
  strong: string;
  muted: string;
  faint: string;
  invert: string;
};

export function canvasPalette(element?: Element | null): CanvasPalette {
  const style = getComputedStyle(element ?? document.documentElement);
  const token = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    ground: token('--sl-color-gray-6', '#141518'),
    grid: token('--sl-color-gray-5', '#242529'),
    ink: token('--sl-color-white', '#f5f5f6'),
    strong: token('--sl-color-gray-1', '#dedee0'),
    muted: token('--sl-color-gray-2', '#b3b4b8'),
    faint: token('--sl-color-gray-3', '#7b7d83'),
    invert: token('--sl-color-black', '#08090b')
  };
}

/**
 * Canvases are painted imperatively, so switching theme would otherwise leave
 * the previous palette on screen until something else forced a redraw. This
 * changes on every theme switch and belongs in a drawing effect's dependencies.
 */
export function useTheme() {
  const [theme, setTheme] = useState(() =>
    typeof document === 'undefined' ? 'dark' : document.documentElement.dataset.theme ?? 'dark'
  );
  useEffect(() => {
    const observer = new MutationObserver(() =>
      setTheme(document.documentElement.dataset.theme ?? 'dark')
    );
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => observer.disconnect();
  }, []);
  return theme;
}
