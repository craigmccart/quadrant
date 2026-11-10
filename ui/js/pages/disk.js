// Disk Map: scan a drive or folder once, then drill in and out instantly.

import { api, onProgress } from "../api.js";
import { h, icon, fill } from "../dom.js";
import * as fmt from "../format.js";
import { setBusy } from "../store.js";
import { button, pageHead, emptyState, busyPanel, stats, meter, fullnessTone, chip, revealButton, showError } from "../ui.js";

// A fixed colour per kind, so a kind keeps its colour whatever else is shown.
const KIND_COLOUR = {
  Video: "--kind-video",
  Images: "--kind-images",
  Archives: "--kind-archives",
  Apps: "--kind-apps",
  Documents: "--kind-documents",
  Code: "--kind-code",
  Audio: "--kind-audio",
  System: "--kind-system",
  Other: "--kind-other",
};

function mount(root) {
  let summary = null;
  let listing = null;
  let running = false;

  const drivesHost = h("div", { class: "drive-cards" });
  const busyHost = h("div");
  const body = h("div", { class: "stack" });

  async function loadDrives() {
    let drives = [];
    try {
      drives = await api.drives();
    } catch (err) {
      showError(err);
    }
    fill(
      drivesHost,
      drives.map((d) => {
        const used = d.totalBytes ? (d.totalBytes - d.freeBytes) / d.totalBytes : 0;
        return h(
          "button",
          { type: "button", class: "drive-card", disabled: running, onclick: () => scan(d.mount) },
          h("div", { class: "drive-top" }, h("span", { class: "drive-name" }, d.mount.replace(/\\$/, ""), d.label && h("span", { class: "drive-label" }, d.label)), d.kind !== "Unknown" && chip(d.kind)),
          meter(used, fullnessTone(used)),
          h("div", { class: "drive-card-foot num" }, `${fmt.bytes(d.freeBytes)} free of ${fmt.bytes(d.totalBytes)}`)
        );
      }),
      h(
        "button",
        {
          type: "button",
          class: "drive-card pick",
          disabled: running,
          onclick: async () => {
            try {
              const picked = await api.pickFolder();
              if (picked) scan(picked);
            } catch (err) {
              showError(err);
            }
          },
        },
        icon("folder-open"),
        "Choose a folder"
      )
    );
  }

  function typeBreakdown() {
    const total = summary.byType.reduce((n, t) => n + t.bytes, 0) || 1;
    const kinds = summary.byType.filter((t) => t.bytes > 0);
    const colour = (kind) => `var(${KIND_COLOUR[kind] || KIND_COLOUR.Other})`;
    return h(
      "section",
      { class: "panel panel-pad" },
      h("h2", { class: "section-title" }, "What it is made of"),
      h(
        "div",
        { class: "typebar", role: "img", "aria-label": kinds.map((t) => `${t.kind} ${fmt.bytes(t.bytes)}`).join(", ") },
        kinds.map((t) => h("span", { title: `${t.kind}: ${fmt.bytes(t.bytes)}, ${fmt.plural(t.fileCount, "file")}`, style: { "--share": String(t.bytes / total), "--c": colour(t.kind) } }))
      ),
      h(
        "ul",
        { class: "typelegend" },
        kinds.map((t) => h("li", null, h("i", { style: { "--c": colour(t.kind) } }), h("b", null, t.kind), h("span", { class: "muted num" }, `${fmt.bytes(t.bytes)} · ${fmt.percent(t.bytes / total)}`)))
      )
    );
  }

  function crumbs() {
    // Split the current path into clickable steps below the scanned root.
    const rootPath = summary.root.replace(/\\$/, "");
    const rest = listing.path.replace(/\\$/, "").slice(rootPath.length).split("\\").filter(Boolean);
    const steps = [{ label: summary.root, path: summary.root }];
    let acc = rootPath;
    for (const part of rest) {
      acc += `\\${part}`;
      steps.push({ label: part, path: acc });
    }
    return h(
      "nav",
      { class: "crumbs", "aria-label": "Folder path" },
      steps.map((step, i) => [
        i > 0 && h("span", { class: "sep" }, "›"),
        i === steps.length - 1 ? h("span", { class: "here" }, step.label) : h("button", { type: "button", onclick: () => open(step.path) }, step.label),
      ])
    );
  }

  function dirRows() {
    return h(
      "div",
      { class: "dir-list" },
      listing.entries.map((e) => {
        const canOpen = e.isDir && e.hasChildren;
        return h(
          "div",
          {
            class: `dir-row row ${canOpen ? "can-open" : ""}`,
            role: canOpen ? "button" : undefined,
            tabindex: canOpen ? 0 : undefined,
            onclick: canOpen ? () => open(e.path) : undefined,
            onkeydown: canOpen ? (ev) => (ev.key === "Enter" || ev.key === " ") && (ev.preventDefault(), open(e.path)) : undefined,
          },
          icon(e.isDir ? "folder" : "file"),
          h("span", { class: "dir-name", title: e.path || e.name }, e.name, e.denied && h("span", { class: "subtle", title: "Windows would not let Quadrant read all of this folder" }, " (partly unread)")),
          meter(e.percent / 100, "quiet"),
          h("span", { class: "dir-size" }, fmt.bytes(e.bytes)),
          h("span", { class: "dir-pct num" }, `${e.percent < 1 && e.bytes > 0 ? "<1" : Math.round(e.percent)}%`),
          h("span", { class: "dir-files num" }, fmt.plural(e.fileCount, "file")),
          e.path ? h("span", { class: "row-actions" }, revealButton(e.path)) : h("span")
        );
      })
    );
  }

  function render() {
    for (const card of drivesHost.querySelectorAll("button")) card.disabled = running;
    if (running) return;
    if (!summary || !listing) {
      fill(body, emptyState({ icon: "disk", title: "See where the space has gone", text: "Choose a drive or a folder above. Quadrant measures every folder once, then lets you step in and out without waiting again. It only reads: nothing is changed." }));
      return;
    }
    fill(
      body,
      stats([
        { value: fmt.bytes(summary.bytes), label: `in ${summary.root}` },
        { value: fmt.count(summary.fileCount), label: "files" },
        { value: fmt.count(summary.dirCount), label: "folders" },
        { value: fmt.elapsed(summary.elapsedMs), label: "to scan" },
      ]),
      typeBreakdown(),
      h(
        "section",
        null,
        h(
          "div",
          { class: "toolbar" },
          button("Up", { icon: "up", size: "sm", disabled: !listing.parent, onClick: () => listing.parent && open(listing.parent) }),
          crumbs(),
          h("span", { class: "toolbar-count num" }, `${fmt.bytes(listing.bytes)} · ${fmt.plural(listing.fileCount, "file")}`)
        ),
        listing.entries.length ? dirRows() : h("p", { class: "muted" }, "This folder is empty.")
      )
    );
  }

  async function open(path) {
    try {
      listing = await api.diskListing(path);
      render();
    } catch (err) {
      showError(err);
    }
  }

  async function scan(path) {
    if (running) return;
    running = true;
    setBusy("disk", true);
    const busy = busyPanel(() => api.cancel("disk"));
    fill(busyHost, busy.el);
    fill(body);
    render();
    const stop = onProgress("disk", busy.update);
    try {
      const next = await api.diskScan(path);
      summary = next;
      listing = await api.diskListing(next.root);
    } catch (err) {
      showError(err);
    } finally {
      stop();
      running = false;
      setBusy("disk", false);
      fill(busyHost);
      render();
    }
  }

  root.append(pageHead("Disk Map", "What is using the space, folder by folder and by type of file."), drivesHost, busyHost, body);
  render();
  loadDrives();
  return { onShow: (_params, first) => !first && !running && loadDrives() };
}

export default { id: "disk", label: "Disk Map", icon: "disk", mount };
