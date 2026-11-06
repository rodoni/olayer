import { createServer } from "vite";
import puppeteer from "puppeteer";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const fixtureRoot = new URL("../../../tests/fixtures/wms/", import.meta.url);
const image = await readFile(new URL("quadrants.png", fixtureRoot));
const scene = await readFile(new URL("scene.json", fixtureRoot));
const requests = [];
const terrainRequests = [];
const elevation = await readFile(new URL("../wcs/elevation.json", fixtureRoot));
const server = await createServer({
  root, server: { host: "127.0.0.1", port: 0, open: false },
  plugins: [{ name: "wms-fixture", configureServer(server) {
    server.middlewares.use((req, res, next) => {
      const url = new URL(req.url ?? "/", "http://localhost");
      if (url.pathname === "/fixture-wms") {
        requests.push(url.searchParams);
        res.setHeader("Content-Type", "image/png");
        res.end(image);
      } else if (url.pathname === "/fixture-wcs") {
        terrainRequests.push(url.searchParams);
        res.setHeader("Content-Type", "application/json");
        res.end(elevation);
      } else if (url.pathname === "/fixture-scene.json") {
        res.setHeader("Content-Type", "application/json");
        res.end(scene);
      } else next();
    });
  } }],
});
let browser;
try {
  await server.listen();
  browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  const page = await browser.newPage();
  page.on("pageerror", error => process.stderr.write(`${error}\n`));
  await page.goto(`${server.resolvedUrls.local[0]}tests/wms.html`);
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("{"), { timeout: 30000 });
  const result = JSON.parse(await page.$eval("#result", element => element.textContent));
  if (!result.ok) throw new Error(result.error);
  if (requests.length !== 1 || requests[0].get("BBOX") !== "-1,-2,1,2") throw new Error("WMS request changed across display modes or used incorrect axis order");
  if (terrainRequests.length !== 1 || terrainRequests[0].get("FORMAT") !== "application/json") throw new Error("WCS terrain must be fetched once as JSON");
  for (const line of result.results) process.stdout.write(`${line}\n`);
  process.stdout.write("One WMS image and one WCS terrain fetch serve all three display modes.\n");
} finally {
  if (browser) await browser.close();
  await server.close();
}
