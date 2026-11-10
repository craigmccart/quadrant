// Health Check: the front door. One pass over space, privacy, speed and
// security, shown on the four-segment dial, with one safe clean-up button.

import { api, onProgress } from "../api.js";
import { h, icon, fill } from "../dom.js";
import * as fmt from "../format.js";
import { app, cleanOptions, emit, on, setBusy } from "../store.js";
import { navigate } from "../router.js";
import { button, link, statusChip, meter, fullnessTone, showError, chip } from "../ui.js";
import { runClean } from "../cleanflow.js";
import { createDial } from "../dial.js";

const LAST_KEY = "quadrant.lastHealth";

const PILLARS = [
  { id: "space", label: "Space", icon: "space", page: "junk", blurb: "Junk files that are safe to clear, and how full your system drive is." },
  { id: "privacy", label: "Privacy", icon: "privacy", page: "privacy", blurb: "Lists Windows keeps of what you opened, searched for and typed." },
  { id: "speed", label: "Speed", icon: "speed", page: "startup", blurb: "Apps that start with Windows, memory in use and time since a restart." },
  { id: "security", label: "Security", icon: "security", page: "security", blurb: "Antivirus, firewall, encryption and other protection settings." },
];

const KIND_LABEL = { junk: "Junk cleaned", privacy: "Privacy traces cleared", large: "Large files removed", duplicates: "Duplicates removed" };

/** The slim version of a report that we remember between runs. */
function slim(report) {
  return { score: report.score, status: report.status, summary: report.summary, pillars: report.pillars, scannedAt: report.scannedAt, elapsedMs: report.elapsedMs };
}

function loadLast() {
  try {
    return JSON.parse(localStorage.getItem(LAST_KEY) || "null");
  } catch {
    return null;
  }
}

function mount(root) {
  let report = null; // a full report from this run
  let last = loadLast(); // the slim one from a previous run
  let running = false;

  const dial = createDial((id) => focusPillar(id));

  // ── Hero ──────────────────────────────────────────────────────────────────
  const headline = h("h1", { class: "hero-headline" });
  const sub = h("p", { class: "hero-sub" });
  const actions = h("div", { class: "hero-actions" });
  const progressText = h("div", { class: "hero-progress-text" });
  const progressBar = h("div", { class: "progress" }, h("div", { class: "progress-fill" }));
  const progress = h("div", { class: "hero-progress", hidden: true, role: "status", "aria-live": "polite" }, progressText, progressBar);

  const hero = h(
    "section",
    { class: "hero" },
    dial.el,
    h("div", { class: "hero-body" }, h("div", { class: "eyebrow" }, "Health Check"), headline, sub, progress, actions)
  );

  // ── Pillars ───────────────────────────────────────────────────────────────
  const pillarEls = {};
  const pillars = h(
    "section",
    { class: "pillars", "aria-label": "The four areas checked" },
    PILLARS.map((def) => {
      const el = h("article", {
        class: "pillar",
        dataset: { pillar: def.id },
        onmouseenter: () => dial.highlight(def.id),
        onmouseleave: () => dial.highlight(null),
      });
      pillarEls[def.id] = el;
      return el;
    })
  );

  function focusPillar(id) {
    dial.highlight(id);
    for (const [key, el] of Object.entries(pillarEls)) el.classList.toggle("is-focus", key === id);
  }

  function renderPillars(data, stale) {
    for (const def of PILLARS) {
      const p = data?.pillars.find((x) => x.id === def.id);
      const head = h("div", { class: "pillar-head" }, h("span", { class: "pillar-icon" }, icon(def.icon)), h("h2", { class: "pillar-label" }, def.label), p && statusChip(p.status));
      if (!p) {
        fill(pillarEls[def.id], head, h("p", { class: "pillar-blurb" }, def.blurb));
        continue;
      }
      fill(
        pillarEls[def.id],
        head,
        h("div", { class: "pillar-figure" }, h("span", { class: "pillar-headline num" }, p.headline), h("span", { class: "pillar-caption" }, p.caption)),
        h(
          "dl",
          { class: "pillar-facts" },
          p.facts.map((f) => h("div", { class: "fact" }, h("dt", null, f.label), h("dd", { class: "num" }, f.value)))
        ),
        link(p.actionLabel, () => navigate(p.page))
      );
      pillarEls[def.id].classList.toggle("is-stale", stale);
    }
  }

  // ── Storage and history ───────────────────────────────────────────────────
  const storageBody = h("div", { class: "drive-list" });
  const historyBody = h("div", { class: "history" });
  const lower = h(
    "div",
    { class: "home-lower" },
    h("section", { class: "panel" }, h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, "Storage"), link("Disk map", () => navigate("disk"))), h("div", { class: "panel-pad" }, storageBody)),
    h("section", { class: "panel" }, h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, "Cleaned so far")), h("div", { class: "panel-pad" }, historyBody))
  );

  async function loadStorage() {
    try {
      const drives = await api.drives();
      if (drives.length === 0) {
        fill(storageBody, h("p", { class: "muted" }, "No drives found."));
        return;
      }
      fill(
        storageBody,
        drives.map((d) => {
          const used = d.totalBytes - d.freeBytes;
          const usedFraction = d.totalBytes ? used / d.totalBytes : 0;
          const letter = d.mount.replace(/\\$/, "");
          return h(
            "div",
            { class: "drive" },
            h(
              "div",
              { class: "drive-top" },
              h("span", { class: "drive-name" }, letter, d.label && h("span", { class: "drive-label" }, d.label)),
              d.kind !== "Unknown" && chip(d.kind),
              h("span", { class: "drive-free num" }, `${fmt.bytes(d.freeBytes)} free of ${fmt.bytes(d.totalBytes)}`)
            ),
            meter(usedFraction, fullnessTone(usedFraction))
          );
        })
      );
    } catch (err) {
      fill(storageBody, h("p", { class: "muted" }, err.message));
    }
  }

  async function loadHistory() {
    try {
      const history = await api.history();
      if (history.entries.length === 0) {
        fill(historyBody, h("p", { class: "muted" }, "Nothing cleaned yet. Each clean-up leaves a short receipt here, with what was removed and how much space it gave back."));
        return;
      }
      const total = fmt.bytesParts(history.totalFreedBytes);
      fill(
        historyBody,
        h("div", { class: "history-total" }, h("span", { class: "history-number num" }, total.value), h("span", { class: "history-unit" }, total.unit), h("span", { class: "history-caption" }, `freed across ${fmt.plural(history.entries.length, "clean-up")}`)),
        h(
          "ul",
          { class: "history-list" },
          history.entries.slice(0, 4).map((e) =>
            h("li", null, h("span", { class: "history-kind" }, KIND_LABEL[e.kind] || "Cleaned"), h("span", { class: "history-when" }, fmt.when(e.at)), h("span", { class: "history-freed num" }, fmt.bytes(e.freedBytes)))
          )
        )
      );
    } catch (err) {
      fill(historyBody, h("p", { class: "muted" }, err.message));
    }
  }

  // ── States ────────────────────────────────────────────────────────────────

  function render() {
    progress.hidden = true;
    hero.dataset.state = report ? "result" : last ? "stale" : "idle";

    if (report) {
      dial.result(report);
      headline.textContent = report.summary;
      fill(sub, `Checked ${fmt.when(report.scannedAt)}, in ${fmt.elapsed(report.elapsedMs)}. Nothing has been changed.`);
      const canClean = report.quickCleanBytes > 0;
      fill(
        actions,
        canClean && button(`Clear ${fmt.bytes(report.quickCleanBytes)} of safe junk`, { variant: "primary", size: "lg", icon: "junk", onClick: quickClean }),
        button("Check again", { size: "lg", icon: "refresh", variant: canClean ? undefined : "primary", onClick: run })
      );
      renderPillars(report, false);
    } else if (last) {
      dial.result(last, true);
      headline.textContent = last.summary;
      fill(sub, `From your last check, ${fmt.when(last.scannedAt)}. Run it again to see where things stand now.`);
      fill(actions, button("Run Health Check", { variant: "primary", size: "lg", icon: "play", onClick: run }));
      renderPillars(last, true);
    } else {
      dial.idle();
      headline.textContent = "See how this PC is doing.";
      fill(sub, "One pass over disk space, privacy traces, startup apps and security settings. It only looks. Nothing is removed or changed until you say so.");
      fill(actions, button("Run Health Check", { variant: "primary", size: "lg", icon: "play", onClick: run }));
      renderPillars(null, false);
    }
  }

  function showProgress(p) {
    progress.hidden = false;
    progressText.textContent = p.message || "Working";
    if (typeof p.fraction === "number") {
      progressBar.classList.remove("indeterminate");
      progressBar.firstChild.style.width = `${Math.round(p.fraction * 100)}%`;
      dial.scanning(p.fraction);
    }
  }

  async function run() {
    if (running) return;
    running = true;
    setBusy("health", true);
    hero.dataset.state = "scanning";
    headline.textContent = "Checking this PC";
    fill(sub, "This usually takes under a minute. You can use the other tools while it runs.");
    fill(actions, button("Stop", { icon: "stop", size: "lg", onClick: () => api.cancel("health") }));
    progressBar.firstChild.style.width = "0%";
    showProgress({ message: "Starting", fraction: 0 });
    const stop = onProgress("health", showProgress);
    try {
      report = await api.healthCheck(cleanOptions());
      last = slim(report);
      try {
        localStorage.setItem(LAST_KEY, JSON.stringify(last));
      } catch {
        // Not being able to remember the result is not worth a message.
      }
      app.health = report;
      emit("health", report);
    } catch (err) {
      showError(err);
    } finally {
      stop();
      running = false;
      setBusy("health", false);
      render();
      loadStorage();
    }
  }

  async function quickClean() {
    if (running || !report) return;
    const chosen = report.junk.categories.filter((c) => report.quickCleanIds.includes(c.id));
    running = true;
    try {
      const done = await runClean("junk", chosen, (p) => {
        hero.dataset.state = "scanning";
        showProgress({ ...p, message: p.message || "Clearing" });
      });
      running = false;
      if (done) {
        // Measure again so the dial shows the PC as it is now.
        await run();
      } else {
        render();
      }
    } catch (err) {
      running = false;
      showError(err);
      render();
    }
  }

  root.append(hero, pillars, lower);
  render();
  loadStorage();
  loadHistory();
  on("cleaned", loadHistory);
  on("removed", loadHistory);

  return {
    onShow() {
      loadHistory();
    },
  };
}

export default { id: "health", label: "Health Check", icon: "health", mount };
