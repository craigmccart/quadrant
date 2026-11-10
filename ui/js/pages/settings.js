// Settings: appearance, how cautious cleaning is, the optional AI key.

import { api } from "../api.js";
import { h, icon, fill } from "../dom.js";
import { app, settings, setSetting } from "../store.js";
import { button, pageHead, segmented, toggle, showError, toast, restartAsAdmin, note, chip } from "../ui.js";

function setting(name, help, control) {
  return h("div", { class: "setting" }, h("div", { class: "setting-text" }, h("div", { class: "setting-name" }, name), help && h("div", { class: "setting-help" }, help)), control);
}

function section(title, ...rows) {
  return h("section", { class: "panel" }, h("div", { class: "panel-head" }, h("h2", { class: "section-title" }, title)), rows);
}

function mount(root) {
  const aiHost = h("div", { class: "setting setting-stack" });

  function renderAi(status) {
    const input = h("input", { type: "password", class: "input mono", placeholder: status.hasKey ? "Paste a new key to replace the saved one" : "Paste your Gemini API key", autocomplete: "off", spellcheck: false, "aria-label": "Gemini API key" });
    const save = async () => {
      if (!input.value.trim()) return;
      try {
        renderAi(await api.aiSetKey(input.value));
        toast("API key saved to Windows Credential Manager.", "good");
      } catch (err) {
        showError(err);
      }
    };
    input.addEventListener("keydown", (e) => e.key === "Enter" && save());
    fill(
      aiHost,
      h(
        "div",
        { class: "setting-text" },
        h("div", { class: "setting-name" }, "Gemini API key ", status.hasKey ? chip(`Saved, ends in ${status.keyHint}`, "good", "check") : chip("Not set")),
        h("div", { class: "setting-help" }, `Optional. With a key, the Junk Cleaner can ask Google Gemini (${status.model}) to explain a scan in plain English. Only category names and sizes are sent, never file names or paths. The key is kept in Windows Credential Manager, not in a file.`)
      ),
      h("div", { class: "key-row" }, input, button("Save", { variant: "primary", onClick: save }), status.hasKey && button("Remove", { icon: "trash", onClick: async () => { try { renderAi(await api.aiClearKey()); toast("API key removed.", "good"); } catch (err) { showError(err); } } })),
      status.legacyKeyFile &&
        !status.hasKey &&
        note(
          "info",
          "A key file from the old Python version of this app (.winscan_key) is sitting next to the app. It is plain text. You can bring that key in here, then delete the file.",
          button("Use that key", { size: "sm", onClick: async () => { try { renderAi(await api.aiImportLegacy()); toast("Key brought in. You can delete the old .winscan_key file now.", "good"); } catch (err) { showError(err); } } })
        )
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
          h("div", { class: "setting-text" }, h("div", { class: "setting-name" }, `Quadrant ${app.version}`), h("div", { class: "setting-help" }, "By sonofg0tham. Quadrant keeps a short history of what it has cleaned in your local app data folder, with counts and sizes only. Nothing is sent anywhere unless you ask Gemini for advice.")),
          h("ul", { class: "credits" }, h("li", null, "Typefaces: Fraunces, Schibsted Grotesk and IBM Plex Mono (SIL Open Font Licence)."), h("li", null, "Icons: Lucide (ISC licence)."), h("li", null, "Built with Rust and Tauri."))
        )
      )
    )
  );

  api.aiStatus().then(renderAi).catch((err) => fill(aiHost, h("p", { class: "muted" }, err.message)));
}

export default { id: "settings", label: "Settings", icon: "settings", mount };
