// Network: every open connection and listening port, and which program owns it.

import { api } from "../api.js";
import { h, fill } from "../dom.js";
import * as fmt from "../format.js";
import { button, pageHead, stats, table, twoLine, searchBox, segmented, showError, chip } from "../ui.js";

const endpoint = (address, port) => (address ? `${address.includes(":") ? `[${address}]` : address}:${port}` : "");

function mount(root) {
  let rows = [];
  let filter = "";
  let view = "all";
  let hideLocal = true;
  const statsHost = h("div");
  const count = h("span", { class: "toolbar-count" });

  const stateTone = (s) => (s === "Established" ? "good" : s === "Listening" ? "info" : "");

  const list = table(
    [
      { label: "Program", sort: (c) => c.process || "~", render: (c) => twoLine(c.process || "Unknown", `PID ${c.pid}`) },
      { label: "Type", tight: true, sort: (c) => c.protocol, render: (c) => c.protocol },
      { label: "This PC", mono: true, sort: (c) => c.localPort, render: (c) => endpoint(c.localAddress, c.localPort) },
      { label: "Other end", mono: true, sort: (c) => c.remoteAddress, render: (c) => endpoint(c.remoteAddress, c.remotePort) },
      { label: "State", tight: true, sort: (c) => c.state, render: (c) => (c.state ? chip(c.state, stateTone(c.state)) : h("span", { class: "subtle" }, "Open port")) },
    ],
    { pageSize: 250, empty: "No connections match." }
  );

  function visible() {
    return rows.filter((c) => {
      if (view === "connected" && c.state !== "Established") return false;
      if (view === "listening" && c.state !== "Listening") return false;
      if (hideLocal && c.loopback) return false;
      if (!filter) return true;
      return `${c.process} ${c.pid} ${c.localAddress}:${c.localPort} ${c.remoteAddress}:${c.remotePort} ${c.state}`.toLowerCase().includes(filter);
    });
  }

  function render() {
    const connected = rows.filter((c) => c.state === "Established");
    const outside = new Set(connected.filter((c) => !c.loopback).map((c) => c.remoteAddress));
    fill(
      statsHost,
      stats([
        { value: fmt.count(connected.length), label: "open connections" },
        { value: fmt.count(outside.size), label: "other computers talked to" },
        { value: fmt.count(rows.filter((c) => c.state === "Listening").length), label: "ports listening" },
        { value: fmt.count(new Set(rows.map((c) => c.pid)).size), label: "programs using the network" },
      ])
    );
    const shown = visible();
    list.setRows(shown);
    count.textContent = `${fmt.count(shown.length)} of ${fmt.count(rows.length)} shown`;
  }

  async function load() {
    try {
      rows = await api.networkList();
      render();
    } catch (err) {
      showError(err);
    }
  }

  const localBtn = button("", { size: "sm", onClick: () => { hideLocal = !hideLocal; label(); render(); } });
  const label = () => (localBtn.textContent = hideLocal ? "Show connections within this PC" : "Hide connections within this PC");
  label();

  root.append(
    pageHead("Network", "Which programs are talking to the network right now, and to where. A snapshot, taken when you refresh.", button("Refresh", { icon: "refresh", variant: "primary", onClick: load })),
    statsHost,
    h(
      "div",
      { class: "toolbar" },
      segmented([{ value: "all", label: "All" }, { value: "connected", label: "Connected" }, { value: "listening", label: "Listening" }], view, (v) => { view = v; render(); }, "Show"),
      searchBox("Search program, address or port", (q) => { filter = q; render(); }),
      localBtn,
      h("span", { class: "toolbar-spacer" }),
      count
    ),
    list.el
  );
  load();
  return { onShow: (_p, first) => !first && load() };
}

export default { id: "network", label: "Network", icon: "network", mount };
