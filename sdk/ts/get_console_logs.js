import puppeteer from 'puppeteer';
import fs from 'fs';

(async () => {
  const browser = await puppeteer.launch({ headless: true });
  const page = await browser.newPage();

  const logs = [];

  page.on('console', (msg) => {
    logs.push(`[CONSOLE] ${msg.text()}`);
  });

  page.on('pageerror', (err) => {
    logs.push(`[ERROR] ${err.toString()}`);
  });

  logs.push('Navigating to http://localhost:3000/demo/index.html...');
  await page.goto('http://localhost:3000/demo/index.html', { waitUntil: 'networkidle2' });

  logs.push('Page loaded, waiting 5 seconds for tiles...');
  await new Promise(resolve => setTimeout(resolve, 5000));

  await browser.close();
  logs.push('Done.');

  fs.writeFileSync('C:\\Users\\rafae\\.gemini\\antigravity-ide\\brain\\eabcfa80-93c6-41f3-a72f-a7886eaafe4f\\scratch\\browser_logs.txt', logs.join('\n'));
  console.log('Saved logs.');
})();
