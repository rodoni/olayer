import { createServer } from "vite";
import puppeteer from "puppeteer";
import { createCanvas } from "canvas";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const scene = await readFile(new URL("../../../tests/fixtures/depth/scene.json", import.meta.url));
const images = new Map([0, 255].map(alpha => {
  const canvas = createCanvas(1, 1); const context = canvas.getContext("2d");
  context.fillStyle = `rgba(0,0,255,${alpha / 255})`; context.fillRect(0, 0, 1, 1);
  return [alpha, canvas.toBuffer("image/png")];
}));
const counts = { wfs: 0, wms: 0 };
const server = await createServer({ root: fileURLToPath(new URL("../", import.meta.url)), server: { host: "127.0.0.1", port: 0, open: false },
  plugins: [{ name: "depth-fixtures", configureServer(server) { server.middlewares.use((req, res, next) => {
    const url = new URL(req.url ?? "/", "http://localhost");
    res.setHeader("Content-Type", "application/json");
    if (url.pathname === "/fixture-scene.json") res.end(scene);
    else if (url.pathname === "/fixture-wcs") {
      const height = Number(url.searchParams.get("height"));
      res.end(JSON.stringify({ width: 2, height: 2, samples: [height, height, height, height], nodata: -9999 }));
    } else if (url.pathname === "/fixture-wfs") {
      counts.wfs++; const height = Number(url.searchParams.get("height"));
      res.end(JSON.stringify({ type: "FeatureCollection", features: [{ type: "Feature", geometry: { type: "Point", coordinates: [0, 0, height] }, properties: {} }] }));
    } else if (url.pathname === "/fixture-wms") {
      counts.wms++; res.setHeader("Content-Type", "image/png"); res.end(images.get(Number(url.searchParams.get("alpha"))));
    } else { res.removeHeader("Content-Type"); next(); }
  }); } }],
});
let browser;
try {
  await server.listen();
  browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  const page = await browser.newPage(); page.on("pageerror", error => process.stderr.write(`${error}\n`));
  await page.goto(`${server.resolvedUrls.local[0]}tests/depth.html`);
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("{"), { timeout: 30000 });
  const result = JSON.parse(await page.$eval("#result", element => element.textContent));
  if (!result.ok) throw new Error(result.error);
  if (counts.wfs !== 2 || counts.wms !== 2) throw new Error("View changes must reuse loaded WFS/WMS data");
  for (const line of result.results) process.stdout.write(`${line}\n`);
} finally { if (browser) await browser.close(); await server.close(); }
