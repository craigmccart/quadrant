// Programs: what is installed, how big it is, and a way to uninstall it.

import { api } from "../api.js";
import { h } from "../dom.js";
import * as fmt from "../format.js";
import { button, pageHead, table, twoLine, searchBox, showError, confirm, toast } from "../ui.js";

function mount(root) {
  let programs = [];
  let filter = "";
  const count = h("span", { class: "toolbar-count" });

  const visible = () => programs.filter((p) => !filter || p.name.toLowerCase().includes(filter) || p.publisher.toLowerCase().includes(filter));

  const list = table(
    [
      { label: "Program", sort: (p) => p.name, render: (p) => twoLine(p.name, p.publisher), wrap: true },
      { label: "Version", sort: (p) => p.version, render: (p) => p.version, mono: true },
      { label: "Installed", tight: true, sort: (p) => p.installedOn || "", descFirst: true, render: (p) => (p.installedOn ? fmt.date(Date.parse(p.installedOn) / 1000) : "") },
      { label: "Size", tight: true, align: "right", mono: true, sort: (p) => p.sizeBytes || 0, descFirst: true, render: (p) => (p.sizeBytes ? fmt.bytes(p.sizeBytes) : "") },
      {
        label: "",
        tight: true,
        render: (p) => p.canUninstall && h("div", { class: "row-actions" }, button("Uninstall", { size: "sm", icon: "uninstall", onClick: () => uninstall(p) })),
      },
    ],
    { sortBy: 0, pageSize: 200, empty: "No programs match." }
  );

  function render() {
    const rows = visible();
    list.setRows(rows);
    const size = programs.reduce((n, p) => n + (p.sizeBytes || 0), 0);
    count.textContent = rows.length === programs.length ? `${fmt.plural(programs.length, "program")} · ${fmt.bytes(size)} reported` : `${fmt.count(rows.length)} of ${fmt.plural(programs.length, "program")}`;
  }

  async function uninstall(p) {
    const ok = await confirm({
      title: `Uninstall ${p.name}?`,
      body: h("p", null, "This starts the uninstaller that came with the program. It opens in its own window and will ask you to confirm there. Quadrant does not remove anything itself."),
      confirmLabel: "Start uninstaller",
      icon: "uninstall",
    });
    if (!ok) return;
    try {
      await api.programUninstall(p.id);
      toast(`The uninstaller for ${p.name} has started. Refresh this list once it has finished.`, "info");
    } catch (err) {
      showError(err);
    }
  }

  async function load() {
    try {
      programs = await api.programsList();
      render();
    } catch (err) {
      showError(err);
    }
  }

  root.append(
    pageHead(
      "Programs",
      "Everything registered with Windows as installed. Sort by size to find the heavy ones.",
      button("Windows installed apps", { icon: "external", onClick: () => api.openKnown("installed-apps").catch(showError) }),
      button("Refresh", { icon: "refresh", onClick: load })
    ),
    h("div", { class: "toolbar" }, searchBox("Search by name or publisher", (q) => { filter = q; render(); }), h("span", { class: "toolbar-spacer" }), count),
    list.el
  );
  load();
  return { onShow: (_p, first) => !first && load() };
}

export default { id: "programs", label: "Programs", icon: "programs", mount };
