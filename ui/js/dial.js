// The health dial: one ring made of four arcs (Space, Privacy, Speed,
// Security), each filled from its own score. Quadrant's signature piece.

import { h, s, prefersReducedMotion } from "./dom.js";
import * as fmt from "./format.js";

const SIZE = 240;
const C = SIZE / 2;
const R = 102;
const GAP_DEG = 9; // empty space between neighbouring arcs
const ORDER = ["space", "privacy", "speed", "security"];

function point(angleDeg, radius = R) {
  const a = ((angleDeg - 90) * Math.PI) / 180;
  return [C + radius * Math.cos(a), C + radius * Math.sin(a)];
}

/** One quarter of the ring, clockwise from 12 o'clock, minus half the gap at each end. */
function arcPath(index) {
  const start = index * 90 + GAP_DEG / 2;
  const end = (index + 1) * 90 - GAP_DEG / 2;
  const [x1, y1] = point(start);
  const [x2, y2] = point(end);
  return `M${x1.toFixed(2)} ${y1.toFixed(2)}A${R} ${R} 0 0 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

export function createDial(onHover) {
  const arcs = {};
  const svg = s("svg", { viewBox: `0 0 ${SIZE} ${SIZE}`, class: "dial-svg", "aria-hidden": "true" });

  // A faint ring of ticks inside the arcs, like the face of an instrument.
  svg.append(s("circle", { class: "dial-ticks", cx: C, cy: C, r: R - 19, fill: "none", "stroke-dasharray": "1.5 7.19" }));

  ORDER.forEach((id, i) => {
    const d = arcPath(i);
    const track = s("path", { class: "dial-track", d, fill: "none" });
    // pathLength=100 lets the score be the dash offset directly.
    const value = s("path", { class: "dial-value", d, fill: "none", pathLength: 100, "stroke-dasharray": "100 100", "stroke-dashoffset": 100 });
    // A wide invisible stroke makes the arc easy to point at.
    const hit = s("path", { class: "dial-hit", d, fill: "none" });
    hit.addEventListener("mouseenter", () => onHover?.(id));
    hit.addEventListener("mouseleave", () => onHover?.(null));
    const group = s("g", { class: "dial-arc", "data-pillar": id }, track, value, hit);
    arcs[id] = { group, value };
    svg.append(group);
  });

  const number = h("div", { class: "dial-number num" }, "–");
  const label = h("div", { class: "dial-label" }, "Not checked yet");
  const el = h("div", { class: "dial", dataset: { state: "idle" } }, svg, h("div", { class: "dial-centre" }, number, label));

  let countTimer = null;

  function countTo(target) {
    cancelAnimationFrame(countTimer);
    if (prefersReducedMotion()) {
      number.textContent = String(target);
      return;
    }
    const started = performance.now();
    const duration = 900;
    const tick = (t) => {
      const k = Math.min(1, (t - started) / duration);
      const eased = 1 - Math.pow(1 - k, 3);
      number.textContent = String(Math.round(target * eased));
      if (k < 1) countTimer = requestAnimationFrame(tick);
    };
    countTimer = requestAnimationFrame(tick);
  }

  return {
    el,

    /** Nothing checked yet. */
    idle() {
      cancelAnimationFrame(countTimer);
      el.dataset.state = "idle";
      number.textContent = "–";
      label.textContent = "Not checked yet";
      for (const id of ORDER) {
        arcs[id].value.setAttribute("stroke-dashoffset", 100);
        // An empty arc with round ends would still draw a dot, so hide it.
        arcs[id].value.style.opacity = "0";
        arcs[id].group.removeAttribute("data-status");
      }
    },

    /** A check is running. `fraction` is 0 to 1 when known. */
    scanning(fraction) {
      cancelAnimationFrame(countTimer);
      el.dataset.state = "scanning";
      number.textContent = typeof fraction === "number" ? fmt.percent(fraction) : "…";
      label.textContent = "Checking";
      for (const id of ORDER) {
        arcs[id].group.removeAttribute("data-status");
        arcs[id].value.style.opacity = "1";
      }
    },

    /** Show a result. `stale` dims it (a result from a previous run). */
    result(report, stale = false) {
      el.dataset.state = stale ? "stale" : "result";
      label.textContent = fmt.statusWord(report.status);
      label.dataset.status = report.status;
      countTo(report.score);
      report.pillars.forEach((pillar, i) => {
        const arc = arcs[pillar.id];
        if (!arc) return;
        arc.group.setAttribute("data-status", pillar.status);
        // Stagger the four arcs so the ring fills in clockwise.
        arc.value.style.transitionDelay = `${i * 110}ms`;
        const score = pillar.status === "unknown" ? 0 : pillar.score;
        // Read the layout once so the transition runs from the old value.
        void arc.value.getBoundingClientRect();
        arc.value.style.opacity = score > 0 ? "1" : "0";
        arc.value.setAttribute("stroke-dashoffset", 100 - score);
      });
    },

    /** Highlight one arc (when its pillar is hovered), or none. */
    highlight(id) {
      if (id) el.dataset.focus = id;
      else delete el.dataset.focus;
    },
  };
}
