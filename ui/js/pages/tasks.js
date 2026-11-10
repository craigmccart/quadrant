// Scheduled Tasks: background jobs that did not come with Windows.

import { api } from "../api.js";
import { h, fill } from "../dom.js";
import * as fmt from "../format.js";
import { button, pageHead, stats, table, twoLine, toggle, showError, searchBox, chip } from "../ui.js";

// schtasks reports results as numbers. These are the ones worth naming.
const RESULTS = { "0": "Succeeded", "1": "Failed", "267009": "Running now", "267011": "Has not run yet", "267014": "Stopped by user" };
const blank = (v) => (!v || v === "N/A" ? "" : v);

function mount(root) {
  let tasks = [];
  let filter = "";
  const statsHost = h("div");

  const list = table(
    [
      {
        label: "On",
        tight: true,
        sort: (t) => (t.enabled ? 0 : 1),
        render: (t) => toggle(t.enabled, (enabled, input) => change(t, enabled, input), `${t.name} is enabled`),
      },
      { label: "Task", sort: (t) => t.name, render: (t) => twoLine(t.name, t.action, "mono"), wrap: true },
      { label: "From", sort: (t) => t.author, render: (t) => blank(t.author) },
      { label: "State", tight: true, sort: (t) => t.status, render: (t) => chip(t.status || "Unknown", t.status === "Running" ? "info" : t.enabled ? "good" : "") },
      { label: "Next run", tight: true, render: (t) => blank(t.nextRun) },
      { label: "Last result", tight: true, sort: (t) => t.lastResult, render: (t) => RESULTS[t.lastResult] || (blank(t.lastResult) && `Code ${t.lastResult}`) },
    ],
    { sortBy: 1, pageSize: 200, rowClass: (t) => (t.enabled ? "" : "dim"), empty: "No scheduled tasks from other software were found." }
  );

  function render() {
    const enabled = tasks.filter((t) => t.enabled).length;
    fill(statsHost, stats([{ value: fmt.count(tasks.length), label: "tasks from other software" }, { value: fmt.count(enabled), label: "enabled" }, { value: fmt.count(tasks.length - enabled), label: "switched off" }]));
    list.setRows(tasks.filter((t) => !filter || t.name.toLowerCase().includes(filter) || t.author.toLowerCase().includes(filter) || t.action.toLowerCase().includes(filter)));
  }

  async function change(task, enabled, input) {
    try {
      tasks = await api.taskSet(task.path, enabled);
    } catch (err) {
      input.checked = !enabled;
      showError(err);
      return;
    }
    render();
  }

  async function load() {
    try {
      tasks = await api.tasksList();
      render();
    } catch (err) {
      showError(err);
    }
  }

  root.append(
    pageHead(
      "Scheduled Tasks",
      "Jobs that other software has asked Windows to run in the background, such as updaters. The tasks that are part of Windows are left out.",
      button("Task Scheduler", { icon: "external", onClick: () => api.openKnown("task-scheduler").catch(showError) }),
      button("Refresh", { icon: "refresh", onClick: load })
    ),
    statsHost,
    h("div", { class: "toolbar" }, searchBox("Search tasks", (q) => { filter = q; render(); })),
    list.el
  );
  load();
}

export default { id: "tasks", label: "Scheduled Tasks", icon: "tasks", mount };
