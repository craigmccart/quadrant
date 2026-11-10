// Which page is showing. Pages are built once, the first time they are
// opened, and then kept, so a scan started on one page carries on (and keeps
// its results) while you look at another.

import { h } from "./dom.js";
import { emit } from "./store.js";

const pages = new Map(); // id -> { def, el, api }
let host = null;
let current = null;

/**
 * Register the pages.
 *   def: { id, label, icon, group, mount(root, params) -> { onShow?(params), onHide?() } }
 */
export function setup(hostEl, defs) {
  host = hostEl;
  for (const def of defs) pages.set(def.id, { def, el: null, api: null });
}

export const currentPage = () => current;

/** Show a page. `params` is passed to the page's onShow (and mount). */
export function navigate(id, params) {
  const entry = pages.get(id);
  if (!entry) return;

  if (current && current !== id) {
    const prev = pages.get(current);
    prev.el.hidden = true;
    prev.api?.onHide?.();
  }

  const first = !entry.el;
  if (first) {
    const inner = h("div", { class: "page-inner" });
    entry.el = h("section", { class: "page", "aria-label": entry.def.label }, inner);
    host.append(entry.el);
    entry.api = entry.def.mount(inner, params) || {};
  }

  entry.el.hidden = false;
  if (current !== id) {
    // Replay the small entrance animation each time a page is opened.
    entry.el.firstChild.classList.remove("page-enter");
    void entry.el.firstChild.offsetWidth;
    entry.el.firstChild.classList.add("page-enter");
  }
  current = id;
  entry.api?.onShow?.(params, first);
  emit("navigate", id);
}
