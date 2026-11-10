// Duplicates: files with identical contents. Keep one copy of each, send
// the rest to the Recycle Bin.

import { api, onProgress } from "../api.js";
import { h, icon, fill } from "../dom.js";
import * as fmt from "../format.js";
import { app, settings, on, setBusy } from "../store.js";
import { button, pageHead, emptyState, busyPanel, stats, checkbox, revealButton, folderField, select, showError, note, toast, chip } from "../ui.js";
import { removeFiles } from "../removeflow.js";

const KB = 1024;
const MB = KB * KB;
const SIZES = [
  { value: 100 * KB, label: "100 KB or more" },
  { value: MB, label: "1 MB or more" },
  { value: 10 * MB, label: "10 MB or more" },
  { value: 100 * MB, label: "100 MB or more" },
];
const PAGE = 60;

function mount(root) {
  let scan = null;
  let running = false;
  let shown = PAGE;
  const selected = new Set();

  const folder = folderField(app.homeDir ? `${app.homeDir}\\Documents` : "C:\\");
  const minSize = select(SIZES, MB, "Smallest file to compare");
  const scanBtn = button("Find duplicates", { variant: "primary", icon: "search", onClick: runScan });
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
  const groupsHost = h("div", { class: "dup-groups" });
  const removeBtn = button("", { variant: "primary", onClick: remove });

  const allFiles = () => scan.groups.flatMap((g) => g.files);
  const picks = () => allFiles().filter((f) => selected.has(f.id));

  /** Tick every copy except one in each group. `keep` picks which survives. */
  function autoSelect(keep) {
    selected.clear();
    for (const g of scan.groups) {
      const candidates = g.files.filter((f) => !f.protected);
      if (candidates.length < 2 && candidates.length === g.files.length) continue;
      // Files arrive oldest first.
      const ordered = [...g.files].sort((a, b) => (a.modified || 0) - (b.modified || 0));
      const survivor = keep === "newest" ? ordered[ordered.length - 1] : ordered[0];
      for (const f of g.files) if (f.id !== survivor.id && !f.protected) selected.add(f.id);
    }
    renderGroups();
    renderToolbar();
  }

  function renderToolbar() {
    const chosen = picks();
    const bytes = chosen.reduce((n, f) => n + f.bytes, 0);
    removeBtn.disabled = chosen.length === 0;
    fill(
      removeBtn,
      icon(settings.recycle ? "recycle" : "trash"),
      chosen.length === 0 ? "Select copies to remove" : `${settings.recycle ? "Move" : "Delete"} ${fmt.plural(chosen.length, "copy", "copies")} · ${fmt.bytes(bytes)}`
    );
  }

  function renderSummary() {
    fill(
      summary,
      stats([
        { value: fmt.bytes(scan.wastedBytes), label: "taken up by extra copies" },
        { value: fmt.count(scan.totalGroups), label: scan.totalGroups === 1 ? "set of identical files" : "sets of identical files" },
        { value: fmt.count(scan.filesChecked), label: `files compared in ${fmt.elapsed(scan.elapsedMs)}` },
      ])
    );
  }

  function groupCard(g) {
    const tickable = g.files.filter((f) => !f.protected);
    return h(
      "article",
      { class: "dup-group" },
      h(
        "header",
        { class: "dup-head" },
        h("div", { class: "dup-name" }, g.files[0].name),
        h("div", { class: "dup-meta num" }, `${fmt.plural(g.files.length, "copy", "copies")} · ${fmt.bytes(g.bytes)} each`),
        chip(`${fmt.bytes(g.wastedBytes)} extra`, "attention")
      ),
      h(
        "ul",
        { class: "dup-files" },
        g.files.map((f) =>
          h(
            "li",
            { class: selected.has(f.id) ? "is-going" : "" },
            f.protected
              ? h("span", { class: "lock", title: "In a Windows system location, so Quadrant will not remove it." }, icon("lock", "Protected"))
              : checkbox(selected.has(f.id), (on_, ) => {
                  if (on_) {
                    // One copy must always survive.
                    const others = tickable.filter((x) => x.id !== f.id);
                    const wouldGo = others.every((x) => selected.has(x.id)) && tickable.length === g.files.length;
                    if (wouldGo) {
                      toast("One copy of each file is always kept. Untick another copy first.", "info");
                      renderGroups();
                      return;
                    }
                    selected.add(f.id);
                  } else {
                    selected.delete(f.id);
                  }
                  renderGroups();
                  renderToolbar();
                }, `Remove the copy in ${f.dir}`),
            h("div", { class: "dup-path" }, h("span", { class: "path" }, f.dir), f.name !== g.files[0].name && h("span", { class: "subtle" }, ` as ${f.name}`)),
            h("span", { class: "subtle num" }, fmt.date(f.modified)),
            h("span", { class: "dup-fate" }, selected.has(f.id) ? "Remove" : "Keep"),
            revealButton(f.path)
          )
        )
      )
    );
  }

  function renderGroups() {
    const groups = scan.groups.slice(0, shown);
    fill(
      groupsHost,
      groups.map(groupCard),
      scan.groups.length > shown &&
        h("div", { class: "table-foot dup-more" }, h("span", null, `Showing ${fmt.count(shown)} of ${fmt.count(scan.groups.length)} sets`), button("Show more", { size: "sm", onClick: () => { shown += PAGE; renderGroups(); } }))
    );
  }

  function render() {
    scanBtn.disabled = running;
    if (running) return;
    if (!scan) {
      fill(body, emptyState({ icon: "duplicates", title: "Find identical files", text: "Quadrant compares file contents, not names, so a renamed copy is still found. You choose which copies go. One copy of every file is always kept." }));
      return;
    }
    if (scan.groups.length === 0) {
      fill(body, emptyState({ icon: "good", title: "No duplicates found", text: `Quadrant compared ${fmt.plural(scan.filesChecked, "file")} in ${scan.root} and found no identical copies.` }));
      return;
    }
    fill(
      body,
      summary,
      scan.truncated && note("info", `Showing the ${fmt.count(scan.groups.length)} sets that waste the most space, of ${fmt.count(scan.totalGroups)} found.`),
      h(
        "div",
        null,
        h(
          "div",
          { class: "toolbar" },
          button("Keep the newest of each", { size: "sm", onClick: () => autoSelect("newest") }),
          button("Keep the oldest of each", { size: "sm", onClick: () => autoSelect("oldest") }),
          button("Clear selection", { size: "sm", variant: "quiet", onClick: () => { selected.clear(); renderGroups(); renderToolbar(); } }),
          h("span", { class: "toolbar-spacer" }),
          removeBtn
        ),
        groupsHost
      )
    );
    renderSummary();
    renderGroups();
    renderToolbar();
  }

  async function runScan() {
    if (running) return;
    const path = folder.value();
    if (!path) return;
    running = true;
    setBusy("duplicates", true);
    const busy = busyPanel(() => api.cancel("duplicates"));
    fill(busyHost, busy.el);
    fill(body);
    render();
    const stop = onProgress("duplicates", busy.update);
    try {
      scan = await api.dupScan(path, Number(minSize.value));
      selected.clear();
      shown = PAGE;
    } catch (err) {
      showError(err);
    } finally {
      stop();
      running = false;
      setBusy("duplicates", false);
      fill(busyHost);
      render();
    }
  }

  async function remove() {
    if (running) return;
    const busy = busyPanel(() => api.cancel("remove"));
    try {
      const report = await removeFiles("duplicates", picks(), (p) => {
        fill(busyHost, busy.el);
        busy.update(p);
      });
      if (report) {
        const gone = new Set(report.removedIds);
        for (const g of scan.groups) {
          g.files = g.files.filter((f) => !gone.has(f.id));
          g.wastedBytes = g.bytes * Math.max(0, g.files.length - 1);
        }
        scan.groups = scan.groups.filter((g) => g.files.length > 1);
        scan.totalGroups = scan.groups.length;
        scan.wastedBytes = scan.groups.reduce((n, g) => n + g.wastedBytes, 0);
        for (const id of gone) selected.delete(id);
      }
    } catch (err) {
      showError(err);
    } finally {
      fill(busyHost);
      render();
    }
  }

  root.append(pageHead("Duplicates", "Files with exactly the same contents, wherever they are and whatever they are called."), controls, busyHost, body);
  render();
  on("settings", (key) => key === "recycle" && scan && renderToolbar());
}

export default { id: "duplicates", label: "Duplicates", icon: "duplicates", mount };
