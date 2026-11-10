// Settings: appearance, how cautious cleaning is, the optional local model.

import { api } from "../api.js";
import { h, icon, fill } from "../dom.js";
import { app, settings, setSetting } from "../store.js";
import { button, pageHead, segmented, toggle, restartAsAdmin, note, chip, select } from "../ui.js";

function setting(name, help, control) {
  return h("div", { class: "setting" }, h("div", { class: "setting-text" }, h("div", { class: "setting-name" }, name), help && h("div", { class: "setting-help" }, help)), control);
}

function section(title, ...rows) {
  return h("section", { class: "panel" }, h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, title)), rows);
}

function mount(root) {
  const aiHost = h("div", { class: "setting setting-stack" });

  // The model runs on this PC (Ollama, LM Studio or similar). `status` is
  // null while checking; `problem` is a message when the address is refused.
  function renderAi(status, problem) {
    const input = h("input", { type: "text", class: "input mono", value: settings.aiEndpoint, placeholder: "http://127.0.0.1:11434", autocomplete: "off", spellcheck: false, "aria-label": "Local model address" });
    const check = () => {
      const endpoint = input.value.trim() || "http://127.0.0.1:11434";
      renderAi(null);
      api
        .aiStatus(endpoint)
        .then((s) => {
          setSetting("aiEndpoint", s.endpoint);
          if (!s.models.includes(settings.aiModel)) setSetting("aiModel", s.models[0] || "");
          renderAi(s);
        })
        .catch((err) => renderAi(undefined, err.message));
    };
    input.addEventListener("keydown", (e) => e.key === "Enter" && check());

    let state = chip("Checking");
    if (problem) state = chip("Not allowed", "attention", "attention");
    else if (status && status.reachable && status.models.length) state = chip("Connected", "good", "check");
    else if (status && status.reachable) state = chip("No models installed", "attention", "attention");
    else if (status) state = chip("Not running");

    const picker = status && status.models.length ? select(status.models.map((m) => ({ value: m, label: m })), settings.aiModel, "Model") : null;
    if (picker) picker.addEventListener("change", () => setSetting("aiModel", picker.value));

    fill(
      aiHost,
      h(
        "div",
        { class: "setting-text" },
        h("div", { class: "setting-name" }, "Local model ", state),
        h("div", { class: "setting-help" }, "Optional. The Junk Cleaner can ask a model running on this PC to explain a scan in plain English. It works with Ollama, LM Studio, or anything else that offers the same local API. It is free, and nothing is sent over the internet.")
      ),
      h("div", { class: "key-row" }, input, button("Check", { icon: "refresh", onClick: check })),
      picker && h("div", { class: "key-row" }, h("span", { class: "setting-help ai-label" }, "Model"), picker),
      problem && note("attention", problem),
      status && !status.reachable && note("info", "Nothing answered at that address. Start Ollama (it uses port 11434) or the LM Studio local server (port 1234), then choose Check."),
      status && status.reachable && !status.models.length && note("info", "The model server is running but has no models yet. In Ollama, a small one such as llama3.2:3b is enough for this.")
    );
  }

  root.append(
    pageHead("Settings", "How Quadrant looks and how careful it is."),
    h(
      "div",
      { class: "settings" },
      section(
        "Appearance",
        setting("Theme", "Auto follows the Windows light or dark setting.", segmented([{ value: "auto", label: "Auto", icon: "auto" }, { value: "dark", label: "Dark", icon: "moon" }, { value: "light", label: "Light", icon: "sun" }], settings.theme, (v) => setSetting("theme", v), "Theme")),
        setting("Text size", "Makes everything in Quadrant bigger. Ctrl and + or - also zooms.", segmented([{ value: "standard", label: "Standard" }, { value: "large", label: "Large" }, { value: "larger", label: "Larger" }], settings.text, (v) => setSetting("text", v), "Text size"))
      ),
      section(
        "Cleaning",
        setting("Leave temporary files from the last 24 hours", "A running installer or app may still need its recent temporary files. Recommended on.", toggle(settings.tempMinAgeHours > 0, (on) => setSetting("tempMinAgeHours", on ? 24 : 0), "Leave recent temporary files")),
        setting("Send removed files to the Recycle Bin", "Applies to Large Files and Duplicates, so a mistake can be put right. Switch off to delete them permanently instead.", toggle(settings.recycle, (on) => setSetting("recycle", on), "Use the Recycle Bin"))
      ),
      section("AI advice", aiHost),
      section(
        "Administrator rights",
        setting(
          app.isElevated ? "Running as administrator" : "Running with standard rights",
          app.isElevated ? "Quadrant can clear system folders and change startup items for all users." : "Enough for most things. Clearing Windows system folders and changing items for all users needs administrator rights.",
          app.isElevated ? h("span", { class: "chip good" }, icon("admin"), "Administrator") : button("Restart as administrator", { icon: "shield", onClick: restartAsAdmin })
        )
      ),
      section(
        "About",
        h(
          "div",
          { class: "setting setting-stack" },
          h("div", { class: "setting-text" }, h("div", { class: "setting-name" }, `Quadrant ${app.version}`), h("div", { class: "setting-help" }, "By sonofg0tham. Quadrant keeps a short history of what it has cleaned in your local app data folder, with counts and sizes only. Nothing is sent over the internet. The optional advice comes from a model on this PC.")),
          h("ul", { class: "credits" }, h("li", null, "Typefaces: Fraunces, Schibsted Grotesk and IBM Plex Mono (SIL Open Font Licence)."), h("li", null, "Icons: Lucide (ISC licence)."), h("li", null, "Built with Rust and Tauri."))
        )
      )
    )
  );

  renderAi(null);
  api
    .aiStatus(settings.aiEndpoint)
    .then((s) => {
      if (!s.models.includes(settings.aiModel)) setSetting("aiModel", s.models[0] || "");
      renderAi(s);
    })
    .catch((err) => renderAi(undefined, err.message));
}

export default { id: "settings", label: "Settings", icon: "settings", mount };
