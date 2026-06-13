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

  fs.writeFileSync('C:\\Users\\rafae\\.gemini\\antigravity-ide\\brain\\3fa5120b-0750-4348-8e64-10cbcf6b5cc0\\browser_logs.txt', logs.join('\n'));
  console.log('Saved logs.');
})();
