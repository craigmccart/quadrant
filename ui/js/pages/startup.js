// Startup: what launches with Windows, with a switch for each. Switching
// off works the way Task Manager does it, so it can always be undone.

import { api } from "../api.js";
import { h, fill } from "../dom.js";
import * as fmt from "../format.js";
import { app, on } from "../store.js";
import { button, pageHead, stats, table, twoLine, toggle, showError, note, restartAsAdmin, chip } from "../ui.js";

function mount(root) {
  let items = [];
  const statsHost = h("div");
  const noteHost = h("div");

  const whyLocked = (item) => (!item.canToggle ? "Windows removes this one by itself after it has run once." : "Items for all users need administrator rights to change.");
  const locked = (item) => !item.canToggle || (item.needsAdmin && !app.isElevated);

  const list = table(
    [
      {
        label: "On",
        tight: true,
        sort: (i) => (i.enabled ? 0 : 1),
        render: (item) =>
          toggle(item.enabled, (enabled, input) => change(item, enabled, input), `${item.name} starts with Windows`, locked(item), locked(item) ? whyLocked(item) : undefined),
      },
      { label: "App", sort: (i) => i.name, render: (i) => twoLine(i.name, i.target || i.command, "mono"), wrap: true },
      { label: "Registered in", sort: (i) => i.location, render: (i) => i.location },
      { label: "For", tight: true, sort: (i) => i.scope, render: (i) => chip(i.scope === "user" ? "You" : "All users", "", i.scope === "user" ? "user" : "users") },
    ],
    { sortBy: 1, rowClass: (i) => (i.enabled ? "" : "dim"), empty: "Nothing is set to start with Windows." }
  );

  function render() {
    const enabled = items.filter((i) => i.enabled).length;
    fill(
      statsHost,
      stats([
        { value: fmt.count(enabled), label: enabled === 1 ? "app starts with Windows" : "apps start with Windows" },
        { value: fmt.count(items.length - enabled), label: "switched off" },
      ])
    );
    const needAdmin = !app.isElevated && items.some((i) => i.needsAdmin && i.canToggle);
    fill(noteHost, needAdmin && note("info", "Items that start for all users need administrator rights to change. The rest can be switched here.", button("Restart as administrator", { size: "sm", onClick: restartAsAdmin })));
    list.setRows(items);
  }

  async function change(item, enabled, input) {
    try {
      items = await api.startupSet(item.id, enabled);
    } catch (err) {
      input.checked = !enabled;
      showError(err);
      return;
    }
    render();
  }

  async function load() {
    try {
      items = await api.startupList();
      render();
    } catch (err) {
      showError(err);
    }
  }

  root.append(
    pageHead(
      "Startup",
      "Apps that launch when you sign in. Fewer means a quicker start. Switching one off does not uninstall it, and you can switch it back on here.",
      button("Windows startup settings", { icon: "external", onClick: () => api.openKnown("startup-apps").catch(showError) }),
      button("Refresh", { icon: "refresh", onClick: load })
    ),
    statsHost,
    h("div", { class: "stack" }, noteHost, list.el)
  );
  load();
  on("health", (report) => {
    items = report.startup;
    render();
  });
}

export default { id: "startup", label: "Startup", icon: "startup", mount };
