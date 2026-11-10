// Small DOM helpers. The UI builds everything with these rather than HTML
// strings, so text from the machine (file names, program names, registry
// values) can never be read as markup.

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * Build an element.
 *   h("button", { class: "btn", onclick: fn, disabled: true }, icon("play"), "Scan")
 * Props: `class`, `dataset`, `style` (object, set through CSSOM so the strict
 * Content-Security-Policy is happy), `on<event>` handlers, `aria-*` and other
 * attributes. `false`/`null`/`undefined` children and attributes are skipped.
 */
export function h(tag, props, ...children) {
  const el = document.createElement(tag);
  if (props) {
    for (const [key, value] of Object.entries(props)) {
      if (value === false || value === null || value === undefined) continue;
      if (key === "class") el.className = value;
      else if (key === "dataset") Object.assign(el.dataset, value);
      else if (key === "style") setStyle(el, value);
      else if (key.startsWith("on") && typeof value === "function") {
        el.addEventListener(key.slice(2), value);
      } else if (key in el && !key.startsWith("aria-") && key !== "list" && key !== "form") {
        el[key] = value;
      } else {
        el.setAttribute(key, value === true ? "" : String(value));
      }
    }
  }
  append(el, children);
  return el;
}

export function append(el, children) {
  for (const child of children.flat(Infinity)) {
    if (child === false || child === null || child === undefined) continue;
    el.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return el;
}

/** Replace everything inside `el`. */
export function fill(el, ...children) {
  el.replaceChildren();
  return append(el, children);
}

export function setStyle(el, styles) {
  for (const [name, value] of Object.entries(styles)) {
    if (name.startsWith("--")) el.style.setProperty(name, value);
    else el.style[name] = value;
  }
}

/** An icon from the sprite (ui/img/icons.svg). Decorative unless labelled. */
export function icon(name, label) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("viewBox", "0 0 24 24");
  if (label) {
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", label);
  } else {
    svg.setAttribute("aria-hidden", "true");
  }
  const use = document.createElementNS(SVG_NS, "use");
  use.setAttribute("href", `img/icons.svg#${name}`);
  svg.append(use);
  return svg;
}

/** Build an SVG element (for the dial and charts). */
export function s(tag, attrs, ...children) {
  const el = document.createElementNS(SVG_NS, tag);
  if (attrs) {
    for (const [key, value] of Object.entries(attrs)) {
      if (value === false || value === null || value === undefined) continue;
      el.setAttribute(key, String(value));
    }
  }
  for (const child of children.flat()) {
    if (child) el.append(child);
  }
  return el;
}

/** Run `fn` at most once per animation frame. */
export function rafThrottle(fn) {
  let queued = false;
  let lastArgs;
  return (...args) => {
    lastArgs = args;
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      fn(...lastArgs);
    });
  };
}

export function debounce(fn, ms) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

export const prefersReducedMotion = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;
