// The steps every clean goes through, shared by Health Check, Junk Cleaner
// and Privacy: show exactly what will go, do it, show the receipt.

import { api, onProgress } from "./api.js";
import { h } from "./dom.js";
import * as fmt from "./format.js";
import { app, cleanOptions, emit } from "./store.js";
import { confirm, dialog, ledger, note } from "./ui.js";

function sizeOrCount(category) {
  // Registry-backed privacy items have no size, only a count.
  return category.bytes > 0 ? fmt.bytes(category.bytes) : fmt.plural(category.itemCount, "item");
}

/** Ask before cleaning. Resolves true if the user goes ahead. */
export function confirmClean(area, categories) {
  const totalBytes = categories.reduce((n, c) => n + c.bytes, 0);
  const totalItems = categories.reduce((n, c) => n + c.itemCount, 0);
  const review = categories.filter((c) => c.risk === "review");
  const toBin = categories.filter((c) => c.toRecycleBin);
  const needAdmin = !app.isElevated && categories.some((c) => c.needsAdmin);

  const rows = categories.map((c) => ({
    name: c.label,
    note: c.toRecycleBin ? "Goes to the Recycle Bin" : c.note,
    value: sizeOrCount(c),
  }));
  rows.push({
    name: "Total",
    value: totalBytes > 0 ? fmt.bytes(totalBytes) : fmt.plural(totalItems, "item"),
    total: true,
  });

  const permanent = categories.length - toBin.length;
  const body = [
    h(
      "p",
      null,
      area === "junk"
        ? "This is what will be removed. Anything in use is skipped and left where it is."
        : "This clears the lists below. Your files themselves are not touched."
    ),
    ledger(rows),
    review.length > 0 &&
      note(
        "attention",
        `${review.map((c) => c.label).join(", ")} ${review.length === 1 ? "is" : "are"} marked "check first". Make sure you have looked at what is in there.`
      ),
    needAdmin &&
      note("info", "Some of this needs administrator rights. Without them, protected files are skipped and reported."),
    permanent > 0 && h("p", { class: "subtle" }, "Apart from anything sent to the Recycle Bin, this cannot be undone."),
  ];

  return confirm({
    title: totalBytes > 0 ? `Clear ${fmt.bytes(totalBytes)}?` : `Clear ${fmt.plural(totalItems, "item")}?`,
    body,
    confirmLabel: "Clear now",
    icon: "junk",
    wide: categories.length > 6,
  });
}

/** Show what a clean did, itemised. */
export function showReceipt(report) {
  const rows = report.items.map((item) => ({
    name: item.label,
    note:
      item.skipped > 0
        ? `${fmt.plural(item.removed, "item")} removed, ${fmt.count(item.skipped)} skipped`
        : `${fmt.plural(item.removed, "item")} removed`,
    value: item.freedBytes > 0 ? fmt.bytes(item.freedBytes) : "Cleared",
  }));
  rows.push({ name: "Freed", value: fmt.bytes(report.freedBytes), total: true });

  const nothing = report.removed === 0;
  return dialog({
    title: nothing ? "Nothing was removed" : report.freedBytes > 0 ? `${fmt.bytes(report.freedBytes)} freed` : "All cleared",
    body: [
      h(
        "p",
        null,
        nothing
          ? "Everything selected was in use, protected or already gone."
          : `${fmt.plural(report.removed, "item")} removed in ${fmt.elapsed(report.elapsedMs)}.`
      ),
      ledger(rows),
      report.skipped > 0 &&
        note(
          "info",
          `${fmt.plural(report.skipped, "item")} skipped because ${report.skipped === 1 ? "it was" : "they were"} in use, too recent or protected. That is normal. Closing your browsers, or running as administrator, lets more be cleared.`
        ),
    ],
    buttons: [{ label: "Done", value: true, variant: "primary", autofocus: true }],
  });
}

/**
 * Confirm, clean, show the receipt.
 * Returns the report, or null if the user backed out.
 * `onProgressUpdate` gets each progress record while it runs.
 */
export async function runClean(area, categories, onProgressUpdate) {
  if (categories.length === 0) return null;
  if (!(await confirmClean(area, categories))) return null;

  const stop = onProgress("clean", (p) => onProgressUpdate?.(p));
  try {
    const report = await api.cleanRun(area, categories.map((c) => c.id), cleanOptions());
    emit("cleaned", report);
    await showReceipt(report);
    return report;
  } finally {
    stop();
  }
}
