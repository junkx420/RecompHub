import { describe, expect, it } from "vitest";
import { pickNext, type Box } from "./gamepad";

const box = (left: number, top: number): Box => ({ left, top, width: 100, height: 40 });

describe("pickNext", () => {
  // A 3x2 grid of tiles, 120 px apart horizontally, 60 px vertically.
  const [a, b, c, d, e, f] = [box(0, 0), box(120, 0), box(240, 0), box(0, 60), box(120, 60), box(240, 60)];
  const others = [a, c, d, e, f];

  it("moves to the neighbour in line", () => {
    expect(others[pickNext(b, others, "right")]).toBe(c);
    expect(others[pickNext(b, others, "left")]).toBe(a);
    expect(others[pickNext(b, others, "down")]).toBe(e);
  });

  it("returns -1 at the edge", () => {
    expect(pickNext(b, others, "up")).toBe(-1);
  });

  it("prefers the closer row over a far one in line", () => {
    const candidates = [box(0, 400), box(150, 60)];
    expect(pickNext(box(0, 0), candidates, "down")).toBe(1);
  });
});
