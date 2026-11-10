// Drive a running debug build of Quadrant through WebView2's DevTools port.
// Development only: release builds ignore the environment variable that
// opens this port (see src-tauri/src/lib.rs).
//
//   set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223
//   target\debug\Quadrant.exe
//   node tools/cdp.mjs eval "document.title" shot out.png
//
// Steps run in order:
//   eval <js>        evaluate an expression (awaited) and print the result
//   click <selector> click the first element that matches
//   wait <ms>        pause
//   waitfor <js>     poll until the expression is truthy (20 s limit)
//   size <w> <h>     emulate a viewport size (0 0 to clear)
//   shot <file.png>  save a screenshot of the web view
//   console          print console errors seen so far

import { writeFileSync } from "node:fs";

const PORT = process.env.QUADRANT_CDP_PORT || "9223";

const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) {
  console.error("No Quadrant page found on port " + PORT);
  process.exit(1);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});

let nextId = 1;
const pending = new Map();
const consoleErrors = [];

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
  } else if (msg.method === "Runtime.exceptionThrown") {
    const d = msg.params.exceptionDetails;
    consoleErrors.push("EXCEPTION " + (d.exception?.description || d.text));
  } else if (msg.method === "Runtime.consoleAPICalled" && ["error", "warning"].includes(msg.params.type)) {
    consoleErrors.push(msg.params.type.toUpperCase() + " " + msg.params.args.map((a) => a.value ?? a.description).join(" "));
  } else if (msg.method === "Log.entryAdded" && msg.params.entry.level === "error") {
    consoleErrors.push("LOG " + msg.params.entry.text);
  }
};

function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  }
  return result.result.value;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");

const args = process.argv.slice(2);
let failed = false;
try {
  for (let i = 0; i < args.length; i++) {
    const step = args[i];
    if (step === "eval") {
      const value = await evaluate(args[++i]);
      console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
    } else if (step === "click") {
      const selector = args[++i];
      const ok = await evaluate(
        `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return false; el.scrollIntoView({block: "center"}); el.click(); return true; })()`
      );
      if (!ok) throw new Error("Nothing matches " + selector);
    } else if (step === "wait") {
      await sleep(Number(args[++i]));
    } else if (step === "waitfor") {
      const expression = args[++i];
      const deadline = Date.now() + 20000;
      while (!(await evaluate(expression))) {
        if (Date.now() > deadline) throw new Error("Timed out waiting for " + expression);
        await sleep(150);
      }
    } else if (step === "size") {
      const width = Number(args[++i]);
      const height = Number(args[++i]);
      if (width && height) {
        await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
      } else {
        await send("Emulation.clearDeviceMetricsOverride");
      }
    } else if (step === "shot") {
      const file = args[++i];
      const { data } = await send("Page.captureScreenshot", { format: "png" });
      writeFileSync(file, Buffer.from(data, "base64"));
      console.log("saved " + file);
    } else if (step === "console") {
      console.log(consoleErrors.length ? consoleErrors.join("\n") : "no console errors");
    } else {
      throw new Error("Unknown step " + step);
    }
  }
} catch (e) {
  failed = true;
  console.error("FAILED: " + e.message);
  if (consoleErrors.length) console.error(consoleErrors.join("\n"));
}
ws.close();
process.exit(failed ? 1 : 0);
