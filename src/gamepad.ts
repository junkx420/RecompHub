/**
 * Controller navigation for the couch: D-pad or left stick moves focus to
 * the nearest control in that direction, A activates it, B goes back,
 * LB/RB switch tabs. Uses the standard Gamepad API mapping.
 */

export type Direction = "up" | "down" | "left" | "right";

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Index of the candidate to move to from `from` in `dir`, or -1. Prefers
 * candidates in line with the current one: distance along the direction
 * plus twice the sideways offset.
 */
export function pickNext(from: Box, candidates: Box[], dir: Direction): number {
  const cx = from.left + from.width / 2;
  const cy = from.top + from.height / 2;
  let best = -1;
  let bestScore = Infinity;
  candidates.forEach((box, index) => {
    const x = box.left + box.width / 2;
    const y = box.top + box.height / 2;
    const along = { up: cy - y, down: y - cy, left: cx - x, right: x - cx }[dir];
    const across = dir === "up" || dir === "down" ? Math.abs(x - cx) : Math.abs(y - cy);
    if (along <= 1) return;
    const score = along + 2 * across;
    if (score < bestScore) {
      bestScore = score;
      best = index;
    }
  });
  return best;
}

const FOCUSABLE =
  'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && el.closest("[inert]") === null;
  });
}

function focus(el: HTMLElement) {
  document.body.classList.add("gamepad-mode");
  el.focus({ preventScroll: true });
  el.scrollIntoView({ block: "nearest", inline: "nearest" });
}

function move(dir: Direction) {
  const all = focusables();
  const current = document.activeElement as HTMLElement | null;
  if (!current || current === document.body || !all.includes(current)) {
    // Start at the first control of the page content, not the tab bar.
    const start = all.find((el) => el.closest("main")) ?? all[0];
    if (start) focus(start);
    return;
  }
  const others = all.filter((el) => el !== current);
  const next = others[pickNext(
    current.getBoundingClientRect(),
    others.map((el) => el.getBoundingClientRect()),
    dir,
  )];
  if (next) focus(next);
}

function back() {
  const button = document.querySelector<HTMLElement>("[data-gamepad-back]");
  if (button) button.click();
}

function switchTab(step: number) {
  const tabs = [...document.querySelectorAll<HTMLElement>('[role="tab"]')];
  const active = tabs.findIndex((t) => t.getAttribute("aria-selected") === "true");
  const next = tabs[(active + step + tabs.length) % tabs.length];
  next?.click();
}

const BUTTON = { a: 0, b: 1, lb: 4, rb: 5, up: 12, down: 13, left: 14, right: 15 };
const FIRST_REPEAT_MS = 350;
const REPEAT_MS = 120;
/** 30 polls per second is plenty for menus, and unlike animation frames
 * it keeps working in windows the browser throttles. */
const POLL_MS = 33;

/** Start polling connected gamepads; returns a function that stops it. */
export function startGamepadNavigation(): () => void {
  const held = new Map<string, number>(); // input -> time it may fire next
  let previousButtons: boolean[] = [];

  function directions(pad: Gamepad): Set<Direction> {
    const pressed = (i: number) => pad.buttons[i]?.pressed === true;
    const [x = 0, y = 0] = pad.axes;
    const dirs = new Set<Direction>();
    if (pressed(BUTTON.up) || y < -0.5) dirs.add("up");
    if (pressed(BUTTON.down) || y > 0.5) dirs.add("down");
    if (pressed(BUTTON.left) || x < -0.5) dirs.add("left");
    if (pressed(BUTTON.right) || x > 0.5) dirs.add("right");
    return dirs;
  }

  function tick() {
    if (!document.hasFocus()) return;
    const now = performance.now();
    const pad = [...navigator.getGamepads()].find((p): p is Gamepad => p !== null && p.connected);
    if (!pad) return;

    const dirs = directions(pad);
    for (const dir of ["up", "down", "left", "right"] as const) {
      if (!dirs.has(dir)) {
        held.delete(dir);
        continue;
      }
      const due = held.get(dir);
      if (due === undefined) {
        move(dir);
        held.set(dir, now + FIRST_REPEAT_MS);
      } else if (now >= due) {
        move(dir);
        held.set(dir, now + REPEAT_MS);
      }
    }

    const buttons = pad.buttons.map((b) => b.pressed);
    const justPressed = (i: number) => buttons[i] === true && previousButtons[i] !== true;
    if (justPressed(BUTTON.a)) {
      const el = document.activeElement as HTMLElement | null;
      if (el && el !== document.body) el.click();
      else move("down");
    }
    if (justPressed(BUTTON.b)) back();
    if (justPressed(BUTTON.lb)) switchTab(-1);
    if (justPressed(BUTTON.rb)) switchTab(1);
    previousButtons = buttons;
  }

  // The mouse takes over again: hide the controller focus ring.
  const onPointer = () => document.body.classList.remove("gamepad-mode");
  window.addEventListener("pointerdown", onPointer);
  const timer = window.setInterval(tick, POLL_MS);
  return () => {
    window.clearInterval(timer);
    window.removeEventListener("pointerdown", onPointer);
  };
}
