/** The small pieces: ring, paws, thought pixels, fold pixels, meter. */

export const Ring = () => <div className="clawd__ring" aria-hidden />;

export const Paws = () => (
  <>
    <div className="clawd__paw clawd__paw--l" aria-hidden />
    <div className="clawd__paw clawd__paw--r" aria-hidden />
  </>
);

export const Thought = () => (
  <div className="clawd__thought" aria-hidden>
    <span />
    <span />
    <span />
  </div>
);

export const Folds = () => (
  <>
    <div className="clawd__fold clawd__fold--l" aria-hidden />
    <div className="clawd__fold clawd__fold--r" aria-hidden />
  </>
);

/**
 * Six blocks, filling on a fixed cadence and cycling once full.
 *
 * Not a progress bar: hooks carry no progress percentage, and inventing one
 * would be claiming knowledge the app does not have (§5.3). The cadence is
 * entirely in CSS, so this component holds no state — which is what keeps the
 * renderer swappable (§7.1).
 */
export const Meter = () => (
  <div className="clawd__meter" aria-hidden>
    {Array.from({ length: 6 }, (_, i) => (
      <span key={i} />
    ))}
  </div>
);
