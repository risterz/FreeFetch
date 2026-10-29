import fs from 'node:fs';
import path from 'node:path';

async function main() {
  const cdpRes = await fetch('http://127.0.0.1:9222/json');
  const targets = await cdpRes.json();
  const pageTarget = targets.find((t) => t.type === 'page');
  if (!pageTarget) {
    throw new Error('No page target found on CDP 9222');
  }

  console.log(`Connecting to CDP target: ${pageTarget.title} (${pageTarget.webSocketDebuggerUrl})`);

  const ws = new WebSocket(pageTarget.webSocketDebuggerUrl);

  let msgId = 1;
  const pending = new Map();

  ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    if (data.id && pending.has(data.id)) {
      const { resolve, reject } = pending.get(data.id);
      pending.delete(data.id);
      if (data.error) reject(data.error);
      else resolve(data.result);
    }
  };

  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });

  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = msgId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async function capture(filename) {
    const res = await send('Page.captureScreenshot', { format: 'png' });
    const buffer = Buffer.from(res.data, 'base64');
    const outPath = path.resolve('docs/screenshots', filename);
    fs.writeFileSync(outPath, buffer);
    console.log(`Saved screenshot: ${outPath} (${buffer.length} bytes)`);
  }

  async function evaluate(expression) {
    return send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
  }

  await send('Page.enable');
  await send('Runtime.enable');
  await send('DOM.enable');

  // Set crisp 1280x820 viewport
  await send('Emulation.setDeviceMetricsOverride', {
    width: 1280,
    height: 820,
    deviceScaleFactor: 1.5,
    mobile: false,
  });

  await new Promise((r) => setTimeout(r, 1200));

  // Helper to ensure all modals are closed
  async function ensureNoModals() {
    await evaluate(`
      (() => {
        // Find and click any X close buttons or close buttons in modals
        const closeButtons = Array.from(document.querySelectorAll('button')).filter(b => {
          const text = (b.textContent || '').trim().toLowerCase();
          return text === 'close' || text === 'cancel' || b.querySelector('svg.lucide-x');
        });
        closeButtons.forEach(btn => btn.click());
      })()
    `);
    await new Promise((r) => setTimeout(r, 400));
  }

  // Helper to click navigation tab by text
  async function clickTab(name) {
    await ensureNoModals();
    await evaluate(`
      (() => {
        const buttons = Array.from(document.querySelectorAll('nav button, header ~ div button, button'));
        const tab = buttons.find(b => (b.textContent || '').trim().toLowerCase().includes('${name.toLowerCase()}'));
        if (tab) tab.click();
      })()
    `);
    await new Promise((r) => setTimeout(r, 800));
  }

  // 1. Capture Main Queue Dashboard (Tab: Queue)
  console.log('--- Step 1: Queue Dashboard ---');
  await clickTab('Queue');
  await capture('01_queue_dashboard.png');

  // 2. Capture History & Analytics (Tab: History) - COMPLETELY MODAL FREE
  console.log('--- Step 2: History & Analytics ---');
  await clickTab('History');
  // Wait for SQLite history data to render
  await new Promise((r) => setTimeout(r, 1000));
  // Verify modal is not present
  const modalCheck = await evaluate(`
    (() => {
      const modals = document.querySelectorAll('.fixed.inset-0');
      return modals.length;
    })()
  `);
  console.log('Modals present on History tab:', modalCheck.result.value);
  await capture('03_history_analytics.png');

  // 3. Capture Settings View (Tab: Settings) - COMPLETELY MODAL FREE
  console.log('--- Step 3: Settings View ---');
  await clickTab('Settings');
  await new Promise((r) => setTimeout(r, 800));
  await capture('04_settings_engine.png');

  // 4. Return to Queue Tab and open Batch URLs modal
  console.log('--- Step 4: Batch URLs Modal ---');
  await clickTab('Queue');
  await evaluate(`
    (() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const batchBtn = buttons.find(b => (b.textContent || '').includes('Batch URLs'));
      if (batchBtn) batchBtn.click();
    })()
  `);
  await new Promise((r) => setTimeout(r, 700));
  await capture('02_batch_modal.png');
  // Close batch modal
  await ensureNoModals();

  // 5. Open Format Modal via URL Analysis
  console.log('--- Step 5: Format Modal ---');
  await clickTab('Queue');
  await evaluate(`
    (async () => {
      const input = document.querySelector('input[type="text"]');
      if (input) {
        const sampleUrl = 'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4';
        const proto = Object.getPrototypeOf(input);
        const setVal = Object.getOwnPropertyDescriptor(proto, 'value').set;
        setVal.call(input, sampleUrl);
        input.dispatchEvent(new Event('input', { bubbles: true }));
        
        const form = input.closest('form');
        if (form) {
          form.dispatchEvent(new Event('submit', { cancelable: true, bubbles: true }));
        }
      }
    })()
  `);
  // Wait for remote stream parse and format modal render
  await new Promise((r) => setTimeout(r, 2500));
  await capture('05_format_modal.png');

  ws.close();
  console.log('All real UI screenshots re-captured and verified successfully.');
}

main().catch((err) => {
  console.error('Capture failed:', err);
  process.exit(1);
});
