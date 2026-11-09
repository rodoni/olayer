import { createServer } from "vite";
import puppeteer from "puppeteer";
import { fileURLToPath } from "node:url";

const server = await createServer({
  root: fileURLToPath(new URL("../", import.meta.url)),
  server: { host: "127.0.0.1", port: 0, open: false },
});
let browser;
try {
  await server.listen();
  browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  const page = await browser.newPage();
  page.on("pageerror", (error) => process.stderr.write(`${error}\n`));
  await page.goto(`${server.resolvedUrls.local[0]}tests/volumetric.html`);
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("{"), { timeout: 30000 });
  const result = JSON.parse(await page.$eval("#result", (element) => element.textContent));
  if (!result.ok) throw new Error(result.error);
  process.stdout.write(`${result.result}\n`);
} finally {
  if (browser) await browser.close();
  await server.close();
}
