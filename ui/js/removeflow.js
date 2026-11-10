// Removing files the user picked in Large Files or Duplicates: confirm with
// the list in front of them, remove, then say plainly what happened.

import { api, onProgress } from "./api.js";
import { h } from "./dom.js";
import * as fmt from "./format.js";
import { emit, settings } from "./store.js";
import { confirm, dialog, ledger, note, toast } from "./ui.js";

const SHOWN = 8;

/**
 * kind: "large" | "duplicates". files: the FoundFile objects to remove.
 * Returns the RemoveReport, or null if the user backed out.
 */
export async function removeFiles(kind, files, onProgressUpdate) {
  if (files.length === 0) return null;
  const toBin = settings.recycle;
  const total = files.reduce((n, f) => n + f.bytes, 0);

  const rows = files.slice(0, SHOWN).map((f) => ({ name: f.name, note: f.dir, value: fmt.bytes(f.bytes) }));
  if (files.length > SHOWN) rows.push({ name: `and ${fmt.plural(files.length - SHOWN, "more file")}`, value: "" });
  rows.push({ name: "Total", value: fmt.bytes(total), total: true });

  const ok = await confirm({
    title: toBin ? `Move ${fmt.plural(files.length, "file")} to the Recycle Bin?` : `Delete ${fmt.plural(files.length, "file")} for good?`,
    body: [
      ledger(rows),
      toBin
        ? note("info", "You can restore these from the Recycle Bin until it is emptied. A file Windows cannot put in the bin (too big, or on a drive without one) is left where it is and listed afterwards.")
        : note("attention", "These will be deleted permanently and cannot be restored. You can switch to using the Recycle Bin in Settings."),
      kind === "duplicates" && h("p", { class: "subtle" }, "At least one copy of every file is always kept."),
    ],
    confirmLabel: toBin ? "Move to Recycle Bin" : "Delete permanently",
    danger: !toBin,
    icon: toBin ? "recycle" : "trash",
    wide: true,
  });
  if (!ok) return null;

  const stop = onProgress("remove", (p) => onProgressUpdate?.(p));
  let report;
  try {
    const ids = files.map((f) => f.id);
    report = kind === "large" ? await api.largeRemove(ids, toBin) : await api.dupRemove(ids, toBin);
  } finally {
    stop();
  }
  emit("removed", report);

  if (report.failed === 0) {
    toast(`${fmt.plural(report.removed, "file")} ${toBin ? "moved to the Recycle Bin" : "deleted"}, ${fmt.bytes(report.freedBytes)} freed.`, "good");
    return report;
  }

  await dialog({
    title: report.removed > 0 ? `${fmt.plural(report.removed, "file")} removed, ${fmt.count(report.failed)} left in place` : "Nothing was removed",
    body: [
      h("p", null, report.removed > 0 ? `${fmt.bytes(report.freedBytes)} freed. These could not be removed:` : "None of the files could be removed:"),
      ledger(report.failures.map((f) => ({ name: f.path, note: f.reason, value: "" }))),
      report.failed > report.failures.length && h("p", { class: "subtle" }, `and ${fmt.count(report.failed - report.failures.length)} more.`),
    ],
    buttons: [{ label: "Done", value: true, variant: "primary", autofocus: true }],
    wide: true,
  });
  return report;
}
