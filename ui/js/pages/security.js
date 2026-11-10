// Security: read-only checks of the protection settings that matter most.

import { api } from "../api.js";
import { h, icon, fill } from "../dom.js";
import { app, on } from "../store.js";
import { button, pageHead, showError, note, emptyState } from "../ui.js";

const ICON = { good: "good", attention: "attention", problem: "problem", unknown: "unknown" };
const ORDER = { problem: 0, attention: 1, unknown: 2, good: 3 };

function mount(root) {
  let checks = null;
  let loading = false;
  const body = h("div", { class: "stack" });
  const headline = h("p", { class: "page-sub" });

  function render() {
    if (!checks) {
      fill(body, emptyState({ icon: "security", title: loading ? "Checking" : "Check this PC", text: "Quadrant reads eight protection settings, including antivirus, firewall and drive encryption. It takes a second or two." }));
      return;
    }
    const known = checks.filter((c) => c.status !== "unknown");
    const passed = known.filter((c) => c.status === "good").length;
    headline.textContent = known.length === 0 ? "None of the checks could be read." : passed === known.length ? `All ${known.length} checks passed.` : `${passed} of ${known.length} checks passed. The ones that need a look are listed first.`;
    // Whatever needs attention goes to the top.
    const sorted = [...checks].sort((a, b) => ORDER[a.status] - ORDER[b.status]);
    fill(
      body,
      h(
        "div",
        { class: "check-list" },
        sorted.map((c) =>
          h(
            "div",
            { class: "check-row" },
            h("div", { class: `check-mark ${c.status}` }, icon(ICON[c.status] || "unknown")),
            h("div", null, h("div", { class: "check-label" }, c.label), h("div", { class: "check-summary selectable" }, c.summary), h("div", { class: "check-detail" }, c.detail)),
            c.fixPage ? button("Open settings", { icon: "external", size: "sm", onClick: () => api.openKnown(c.fixPage).catch(showError) }) : h("span")
          )
        )
      ),
      note("info", "Quadrant only reads these settings. It never changes them. Each button opens the Windows page where you can.")
    );
  }

  async function load() {
    if (loading) return;
    loading = true;
    render();
    try {
      checks = await api.securityChecks();
    } catch (err) {
      showError(err);
    } finally {
      loading = false;
      render();
    }
  }

  const head = pageHead("Security", "", button("Check again", { icon: "refresh", onClick: load }));
  head.querySelector(".page-head-text").append(headline);
  root.append(head, body);

  if (app.health) checks = app.health.security;
  render();
  if (!checks) load();
  on("health", (report) => {
    checks = report.security;
    render();
  });
}

export default { id: "security", label: "Security", icon: "security", mount };
