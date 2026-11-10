// App start-up: build the sidebar, register the pages, show the window.

import { api } from "./api.js";
import { h, icon, fill } from "./dom.js";
import { app, applyAppearance, on, isBusy } from "./store.js";
import { setup, navigate, currentPage } from "./router.js";
import { restartAsAdmin } from "./ui.js";

import health from "./pages/health.js";
import { junk, privacy } from "./pages/cleaner.js";
import duplicates from "./pages/duplicates.js";
import large from "./pages/large.js";
import disk from "./pages/disk.js";
import startup from "./pages/startup.js";
import programs from "./pages/programs.js";
import tasks from "./pages/tasks.js";
import security from "./pages/security.js";
import network from "./pages/network.js";
import system from "./pages/system.js";
import settingsPage from "./pages/settings.js";

// Every tool is one click away in the sidebar. Nothing is tucked in a menu.
const GROUPS = [
  { label: null, pages: [health] },
  { label: "Clean", pages: [junk, privacy, duplicates, large, disk] },
  { label: "Optimise", pages: [startup, programs, tasks] },
  { label: "Inspect", pages: [security, network, system] },
];
const ALL_PAGES = [...GROUPS.flatMap((g) => g.pages), settingsPage];

const navItems = new Map();

function navItem(page) {
  const item = h(
    "button",
    { type: "button", class: "nav-item", onclick: () => navigate(page.id) },
    icon(page.icon),
    h("span", { class: "nav-label" }, page.label)
  );
  navItems.set(page.id, item);
  return item;
}

function buildSidebar() {
  fill(
    document.getElementById("nav"),
    GROUPS.map((group) => [group.label && h("div", { class: "nav-group" }, group.label), group.pages.map(navItem)])
  );

  const adminChip = app.isElevated
    ? h("div", { class: "admin-chip", dataset: { elevated: "true" } }, icon("admin"), "Running as administrator")
    : h(
        "button",
        {
          type: "button",
          class: "admin-chip",
          dataset: { elevated: "false" },
          title: "Quadrant is running with standard rights. Some cleaning and startup changes need administrator rights.",
          onclick: restartAsAdmin,
        },
        icon("shield"),
        "Restart as administrator"
      );

  fill(
    document.getElementById("sidebar-foot"),
    navItem(settingsPage),
    adminChip,
    h("div", { class: "version" }, h("span", null, `Version ${app.version}`), h("span", null, "sonofg0tham"))
  );
}

function markCurrent() {
  const current = currentPage();
  for (const [id, item] of navItems) {
    if (id === current) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
}

function markBusy({ pageId }) {
  const item = navItems.get(pageId);
  if (!item) return;
  item.querySelector(".nav-busy")?.remove();
  if (isBusy(pageId)) {
    item.append(h("span", { class: "nav-busy", role: "img", "aria-label": "Working" }));
  }
}

async function start() {
  applyAppearance();
  try {
    const info = await api.appInfo();
    app.version = info.version;
    app.isElevated = info.isElevated;
    app.homeDir = info.homeDir || "";
  } catch {
    // Carry on with defaults: the pages report their own errors.
  }

  buildSidebar();
  setup(document.getElementById("main"), ALL_PAGES);
  on("navigate", markCurrent);
  on("busy", markBusy);

  // A browser preview can open on a given page: index.html?page=junk
  const wanted = new URLSearchParams(location.search).get("page");
  navigate(ALL_PAGES.some((p) => p.id === wanted) ? wanted : "health");

  // The window starts hidden. Show it once the fonts are in, so the first
  // thing on screen is the finished page.
  await document.fonts.ready;
  requestAnimationFrame(() => api.appReady().catch(() => {}));
}

// For development and automated checks: quadrant.navigate("junk")
window.quadrant = { navigate };

start();
