// System: what this PC is, and how hard it is working right now.

import { api } from "../api.js";
import { h, fill } from "../dom.js";
import * as fmt from "../format.js";
import { button, pageHead, meter, fullnessTone, showError, chip } from "../ui.js";

const POLL_MS = 2000;

function mount(root) {
  const device = h("dl", { class: "kv" });
  const drives = h("div", { class: "drive-list" });
  const cpuValue = h("span", { class: "live-value num" }, "–");
  const ramValue = h("span", { class: "live-value num" }, "–");
  const cpuMeter = h("div");
  const ramMeter = h("div");
  const ramNote = h("div", { class: "subtle num" });
  let timer = null;

  async function load() {
    try {
      const o = await api.systemOverview();
      const rows = [
        ["Device name", o.hostName],
        ["Signed in as", o.userName],
        ["Windows", [o.osName, o.osVersion && `version ${o.osVersion}`, o.osBuild && `build ${o.osBuild}`].filter(Boolean).join(", ")],
        ["Device", [o.manufacturer, o.model].filter(Boolean).join(" ")],
        ["Processor", o.cpuName],
        ["Cores", o.cpuCoresPhysical ? `${o.cpuCoresPhysical} cores, ${o.cpuCoresLogical} threads` : ""],
        ["Graphics", o.gpus.join(", ")],
        ["Memory", o.ramTotalBytes ? fmt.bytes(o.ramTotalBytes) : ""],
        ["Running since", o.bootTime ? `${fmt.when(o.bootTime)} (${fmt.duration(o.uptimeSecs)})` : ""],
        ["Quadrant rights", o.isElevated ? "Administrator" : "Standard"],
      ].filter(([, value]) => value);
      fill(device, rows.map(([k, v]) => [h("dt", null, k), h("dd", null, v)]));
      fill(
        drives,
        o.drives.map((d) => {
          const used = d.totalBytes ? (d.totalBytes - d.freeBytes) / d.totalBytes : 0;
          return h(
            "div",
            { class: "drive" },
            h("div", { class: "drive-top" }, h("span", { class: "drive-name" }, d.mount.replace(/\\$/, ""), d.label && h("span", { class: "drive-label" }, d.label)), d.kind !== "Unknown" && chip(d.kind), d.fileSystem && chip(d.fileSystem), h("span", { class: "drive-free num" }, `${fmt.bytes(d.freeBytes)} free of ${fmt.bytes(d.totalBytes)}`)),
            meter(used, fullnessTone(used))
          );
        })
      );
    } catch (err) {
      showError(err);
    }
  }

  async function sample() {
    try {
      const s = await api.systemLive();
      const ram = s.ramTotalBytes ? s.ramUsedBytes / s.ramTotalBytes : 0;
      cpuValue.textContent = `${Math.round(s.cpuPercent)}%`;
      ramValue.textContent = fmt.percent(ram);
      ramNote.textContent = `${fmt.bytes(s.ramUsedBytes)} of ${fmt.bytes(s.ramTotalBytes)} in use`;
      fill(cpuMeter, meter(s.cpuPercent / 100, fullnessTone(s.cpuPercent / 100)));
      fill(ramMeter, meter(ram, fullnessTone(ram)));
    } catch {
      // A missed sample is not worth a message; the next one will follow.
    }
  }

  root.append(
    pageHead("System", "What this PC is made of, and how busy it is.", button("Refresh", { icon: "refresh", onClick: load })),
    h(
      "div",
      { class: "system-grid" },
      h("section", { class: "panel" }, h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, "This PC")), h("div", { class: "panel-pad" }, device)),
      h(
        "div",
        { class: "stack" },
        h(
          "section",
          { class: "panel" },
          h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, "Right now")),
          h(
            "div",
            { class: "panel-pad live" },
            h("div", null, h("div", { class: "live-top" }, h("span", null, "Processor"), cpuValue), cpuMeter),
            h("div", null, h("div", { class: "live-top" }, h("span", null, "Memory"), ramValue), ramMeter, ramNote)
          )
        ),
        h("section", { class: "panel" }, h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, "Drives")), h("div", { class: "panel-pad" }, drives))
      )
    )
  );
  load();

  return {
    // Only sample while the page is on screen.
    onShow() {
      sample();
      clearInterval(timer);
      timer = setInterval(sample, POLL_MS);
    },
    onHide() {
      clearInterval(timer);
    },
  };
}

export default { id: "system", label: "System", icon: "system", mount };
