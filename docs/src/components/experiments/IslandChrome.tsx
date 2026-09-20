import type { ComponentChildren } from 'preact';

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
