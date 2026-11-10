// Large Files: find the biggest files in a folder and remove the ones you
// no longer want.

import { api, onProgress } from "../api.js";
import { h, icon, fill } from "../dom.js";
import * as fmt from "../format.js";
import { app, settings, on, setBusy } from "../store.js";
import { button, pageHead, emptyState, busyPanel, stats, table, twoLine, checkbox, revealButton, folderField, select, searchBox, showError, note } from "../ui.js";
import { removeFiles } from "../removeflow.js";

const MB = 1024 * 1024;
const SIZES = [
  { value: 50 * MB, label: "50 MB or more" },
  { value: 100 * MB, label: "100 MB or more" },
  { value: 250 * MB, label: "250 MB or more" },
  { value: 500 * MB, label: "500 MB or more" },
  { value: 1024 * MB, label: "1 GB or more" },
];

function mount(root) {
  let scan = null;
  let running = false;
  let filter = "";
  const selected = new Set();

  const folder = folderField(app.homeDir || "C:\\");
  const minSize = select(SIZES, 100 * MB, "Smallest file to list");
  const scanBtn = button("Find large files", { variant: "primary", icon: "search", onClick: runScan });
  const controls = h(
    "div",
    { class: "controls" },
    h("label", { class: "field field-grow" }, h("span", { class: "field-label" }, "Look in"), folder.el),
    h("label", { class: "field" }, h("span", { class: "field-label" }, "Size"), minSize),
    scanBtn
  );
  folder.input.addEventListener("keydown", (e) => e.key === "Enter" && runScan());

  const busyHost = h("div");
  const body = h("div", { class: "stack" });
  const summary = h("div");
  const removeBtn = button("", { variant: "primary", onClick: remove });
  const count = h("span", { class: "toolbar-count" });

  const visible = () => (scan ? scan.files.filter((f) => !filter || f.name.toLowerCase().includes(filter) || f.dir.toLowerCase().includes(filter)) : []);
  const picks = () => scan.files.filter((f) => selected.has(f.id));

  const list = table(
    [
      {
        label: "",
        srLabel: "Select",
        tight: true,
        render: (f) =>
          f.protected
            ? h("span", { class: "lock", title: "Windows needs this file, so Quadrant will not remove it." }, icon("lock", "Protected"))
            : checkbox(selected.has(f.id), (on_) => {
                if (on_) selected.add(f.id);
                else selected.delete(f.id);
                renderToolbar();
              }, `Select ${f.name}`),
      },
      { label: "File", sort: (f) => f.name, render: (f) => twoLine(f.name, f.dir, "mono"), wrap: true },
      { label: "Type", sort: (f) => f.kind, render: (f) => f.kind, tight: true },
      { label: "Changed", sort: (f) => f.modified || 0, descFirst: true, render: (f) => fmt.date(f.modified), tight: true },
      { label: "Size", sort: (f) => f.bytes, descFirst: true, align: "right", mono: true, tight: true, render: (f) => fmt.bytes(f.bytes) },
      { label: "", tight: true, render: (f) => h("div", { class: "row-actions" }, revealButton(f.path)) },
    ],
    { sortBy: 4, descending: true, pageSize: 200, rowClass: (f) => (f.protected ? "dim" : ""), empty: "No files match." }
  );

  function renderToolbar() {
    const chosen = picks();
    const bytes = chosen.reduce((n, f) => n + f.bytes, 0);
    removeBtn.disabled = chosen.length === 0;
    fill(
      removeBtn,
      icon(settings.recycle ? "recycle" : "trash"),
      chosen.length === 0 ? "Select files to remove" : `${settings.recycle ? "Move" : "Delete"} ${fmt.plural(chosen.length, "file")} · ${fmt.bytes(bytes)}`
    );
    const shown = visible().length;
    count.textContent = shown === scan.files.length ? fmt.plural(shown, "file") : `${fmt.count(shown)} of ${fmt.plural(scan.files.length, "file")}`;
  }

  function renderSummary() {
    fill(
      summary,
      stats([
        { value: fmt.bytes(scan.totalBytes), label: `in ${fmt.plural(scan.totalCount, "large file")}` },
        { value: scan.files.length ? fmt.bytes(scan.files[0].bytes) : "0 B", label: "biggest single file" },
        { value: fmt.elapsed(scan.elapsedMs), label: "to scan" },
      ])
    );
  }

  function render() {
    scanBtn.disabled = running;
    if (running) return;
    if (!scan) {
      fill(body, emptyState({ icon: "large", title: "Find what is taking the space", text: "Pick a folder and a size. Quadrant lists the biggest files first, shows where each one lives, and removes only the ones you tick." }));
      return;
    }
    if (scan.files.length === 0) {
      fill(body, emptyState({ icon: "good", title: "No large files here", text: `Nothing in ${scan.root} is ${fmt.bytes(scan.minBytes)} or bigger. Try a smaller size or another folder.` }));
      return;
    }
    fill(
      body,
      summary,
      scan.truncated && note("info", `Showing the ${fmt.count(scan.files.length)} biggest of ${fmt.count(scan.totalCount)} files found.`),
      h("div", null, h("div", { class: "toolbar" }, searchBox("Filter by name or folder", (q) => { filter = q; list.setRows(visible()); renderToolbar(); }), count, h("span", { class: "toolbar-spacer" }), removeBtn), list.el)
    );
    renderSummary();
    list.setRows(visible());
    renderToolbar();
  }

  async function runScan() {
    if (running) return;
    const path = folder.value();
    if (!path) return;
    running = true;
    setBusy("large", true);
    const busy = busyPanel(() => api.cancel("large"));
    fill(busyHost, busy.el);
    fill(body);
    render();
    const stop = onProgress("large", busy.update);
    try {
      scan = await api.largeScan(path, Number(minSize.value));
      selected.clear();
      filter = "";
    } catch (err) {
      showError(err);
    } finally {
      stop();
      running = false;
      setBusy("large", false);
      fill(busyHost);
      render();
    }
  }

  async function remove() {
    if (running) return;
    const busy = busyPanel(() => api.cancel("remove"));
    try {
      const report = await removeFiles("large", picks(), (p) => {
        fill(busyHost, busy.el);
        busy.update(p);
      });
      if (report) {
        const gone = new Set(report.removedIds);
        scan.files = scan.files.filter((f) => !gone.has(f.id));
        scan.totalCount -= report.removed;
        scan.totalBytes -= report.freedBytes;
        for (const id of gone) selected.delete(id);
      }
    } catch (err) {
      showError(err);
    } finally {
      fill(busyHost);
      render();
    }
  }

  root.append(pageHead("Large Files", "The biggest files in a folder, largest first. Nothing is removed unless you tick it."), controls, busyHost, body);
  render();
  on("settings", (key) => key === "recycle" && scan && renderToolbar());
}

export default { id: "large", label: "Large Files", icon: "large", mount };
