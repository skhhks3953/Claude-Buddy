import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { STATES, type ClawdState } from "../state";
import { Clawd } from "./Clawd";

/**
 * These assert the renderer's contract (§7.1) rather than its appearance:
 * that state selects a class, that the optional parts appear for exactly the
 * states that call for them, and that nothing is derived.
 */

const draw = (state: ClawdState, overrides: Partial<Parameters<typeof Clawd>[0]> = {}) => {
  const { container } = render(
    <Clawd state={state} label="Editing App.tsx" labelVisible dragging={false} {...overrides} />,
  );
  return container;
};

const sprite = (c: HTMLElement) => c.querySelector(".clawd")!;

describe("the renderer boundary", () => {
  it("gives every state its own class", () => {
    for (const state of STATES) {
      expect(sprite(draw(state)).className).toContain(`clawd--${state}`);
    }
  });

  it("draws a body for every state", () => {
    for (const state of STATES) {
      expect(draw(state).querySelector(".clawd__body")).not.toBeNull();
    }
  });

  it("rings only the states that are asking for something", () => {
    const ringed = STATES.filter((s) => draw(s).querySelector(".clawd__ring"));
    expect(ringed).toEqual(["needsInput", "needsPermission", "success"]);
  });

  it("drums paws only while a tool is running", () => {
    const pawed = STATES.filter((s) => draw(s).querySelector(".clawd__paw"));
    expect(pawed).toEqual(["working"]);
  });

  it("shows the elapsed meter only on a long task", () => {
    const metered = STATES.filter((s) => draw(s).querySelector(".clawd__meter"));
    expect(metered).toEqual(["longTask"]);
    expect(draw("longTask").querySelectorAll(".clawd__meter span")).toHaveLength(6);
  });

  it("badges the four states that have something to say, and pauses with bars", () => {
    const glyphs = Object.fromEntries(
      STATES.map((s) => [s, draw(s).querySelector(".clawd__badge")?.textContent ?? null]),
    );
    expect(glyphs.needsInput).toBe("?");
    expect(glyphs.needsPermission).toBe("!");
    expect(glyphs.success).toBe("✓");
    expect(glyphs.failed).toBe("×");
    expect(glyphs.idle).toBeNull();
    expect(draw("paused").querySelectorAll(".clawd__bar")).toHaveLength(2);
  });

  it("squints on success and drops brows on failure", () => {
    expect(draw("success").querySelectorAll(".clawd__eye")).toHaveLength(4);
    expect(draw("failed").querySelectorAll(".clawd__brow")).toHaveLength(2);
  });

  it("tilts while held", () => {
    expect(sprite(draw("idle", { dragging: true })).className).toContain("clawd--dragging");
    expect(sprite(draw("idle")).className).not.toContain("clawd--dragging");
  });

  it("shows the chip only when told to", () => {
    expect(draw("idle").querySelector(".chip")!.className).toContain("chip--visible");
    expect(
      draw("idle", { labelVisible: false }).querySelector(".chip")!.className,
    ).not.toContain("chip--visible");
  });

  /**
   * The renderer must not be able to pick its own text. If it ever derives a
   * label, this fails.
   */
  it("says exactly what it is given", () => {
    const c = draw("working", { label: "Running npm test" });
    expect(c.querySelector(".chip")!.textContent).toBe("Running npm test");
  });
});
