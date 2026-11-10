// Renders the README logo lockups (mark + wordmark) to transparent PNGs,
// using the app's own bundled Fraunces so the wordmark matches the sidebar.
//
// Needs a debug build running with the DevTools port open (see AGENTS.md):
//   node tools/build-logo.mjs
// Writes assets/quadrant-logo-dark.png and assets/quadrant-logo-light.png.
// The app's page is replaced while this runs, so close the app afterwards.

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
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result.value;
};

// Dark: for dark backgrounds. Light: for light backgrounds.
const VARIANTS = {
  dark: { ink: "#ece7de", brass: "#d2a85c" },
  light: { ink: "#1c1915", brass: "#86600f" },
};

await send("Emulation.setDeviceMetricsOverride", { width: 900, height: 300, deviceScaleFactor: 4, mobile: false });
await send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } });

for (const [name, c] of Object.entries(VARIANTS)) {
  // Built with DOM calls and CSSOM only: the app's CSP allows no inline styles.
  const box = await evaluate(`(async () => {
    const NS = "http://www.w3.org/2000/svg";
    document.documentElement.style.background = "transparent";
    document.body.replaceChildren();
    document.body.style.background = "transparent";
    document.body.style.margin = "0";
    const wrap = document.createElement("div");
    Object.assign(wrap.style, { display: "inline-flex", alignItems: "center", gap: "18px", padding: "6px 8px 6px 4px" });
    const svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", "0 0 34 34");
    svg.setAttribute("width", "64");
    svg.setAttribute("height", "64");
    const arcs = [
      ["M19.6 4.3A13 13 0 0 1 29.7 14.4", 1],
      ["M29.7 19.6A13 13 0 0 1 19.6 29.7", 0.72],
      ["M14.4 29.7A13 13 0 0 1 4.3 19.6", 0.48],
      ["M4.3 14.4A13 13 0 0 1 14.4 4.3", 0.28],
    ];
    for (const [d, opacity] of arcs) {
      const p = document.createElementNS(NS, "path");
      p.setAttribute("d", d);
      p.setAttribute("fill", "none");
      p.setAttribute("stroke", ${JSON.stringify(c.brass)});
      p.setAttribute("stroke-width", "4.2");
      p.setAttribute("stroke-linecap", "round");
      p.setAttribute("opacity", String(opacity));
      svg.append(p);
    }
    const dot = document.createElementNS(NS, "circle");
    dot.setAttribute("cx", "17");
    dot.setAttribute("cy", "17");
    dot.setAttribute("r", "3.4");
    dot.setAttribute("fill", ${JSON.stringify(c.ink)});
    svg.append(dot);
    const word = document.createElement("div");
    word.textContent = "Quadrant";
    Object.assign(word.style, {
      fontFamily: "Fraunces, Georgia, serif",
      fontWeight: "540",
      fontSize: "52px",
      lineHeight: "64px",
      letterSpacing: "-0.02em",
      color: ${JSON.stringify(c.ink)},
    });
    wrap.append(svg, word);
    document.body.append(wrap);
    await document.fonts.load('540 52px Fraunces');
    await document.fonts.ready;
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const r = wrap.getBoundingClientRect();
    return { x: r.x, y: r.y, width: Math.ceil(r.width), height: Math.ceil(r.height), font: document.fonts.check('540 52px Fraunces') };
  })()`);
  if (!box.font) throw new Error("Fraunces did not load");
  const shot = await send("Page.captureScreenshot", {
    format: "png",
    clip: { x: box.x, y: box.y, width: box.width, height: box.height, scale: 1 },
    captureBeyondViewport: false,
  });
  const file = `assets/quadrant-logo-${name}.png`;
  writeFileSync(file, Buffer.from(shot.data, "base64"));
  console.log(`saved ${file} (${box.width}x${box.height} at 4x)`);
}

ws.close();
