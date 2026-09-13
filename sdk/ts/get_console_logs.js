import puppeteer from 'puppeteer';
import fs from 'fs';

(async () => {
  const browser = await puppeteer.launch({
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox']
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800 });

  const logs = [];

  page.on('console', (msg) => {
    logs.push(`[CONSOLE] ${msg.text()}`);
    console.log(`[CONSOLE] ${msg.text()}`);
  });

  page.on('pageerror', (err) => {
    logs.push(`[ERROR] ${err.toString()}`);
    console.error(`[ERROR] ${err.toString()}`);
  });

  console.log('Navigating to http://localhost:3000/demo/index.html...');
  await page.goto('http://localhost:3000/demo/index.html', { waitUntil: 'networkidle2' });

  console.log('Page loaded. Selecting 3D mode...');
  await page.select('#viewModeSelect', '3D');
  await new Promise(r => setTimeout(r, 1000));

  console.log('Selecting TAWS mode...');
  await page.select('#terrainRenderModeSelect', 'taws');
  await new Promise(r => setTimeout(r, 500));

  console.log('Toggling GPU shader contours...');
  await page.click('#shaderContoursCheckbox');
  await new Promise(r => setTimeout(r, 500));

  console.log('Adjusting hillshade azimuth...');
  await page.$eval('#hillshadeAzimuthRange', el => {
    el.value = '210';
    el.dispatchEvent(new Event('input'));
  });
  await new Promise(r => setTimeout(r, 1000));

  const screenshotPath = process.env.SCREENSHOT_PATH || 'terrain_demo_screenshot.png';
  await page.screenshot({ path: screenshotPath });
  console.log('Saved screenshot to:', screenshotPath);

  await browser.close();
  console.log('Verification completed successfully.');
})();
