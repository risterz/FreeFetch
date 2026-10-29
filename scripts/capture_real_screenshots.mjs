import fs from 'node:fs';
import path from 'node:path';

async function main() {
  const cdpRes = await fetch('http://127.0.0.1:9222/json');
  const targets = await cdpRes.json();
  const pageTarget = targets.find((t) => t.type === 'page');
  if (!pageTarget) {
    throw new Error('No page target found on CDP 9222');
  }

  console.log(`Connected to CDP target: ${pageTarget.title} (${pageTarget.webSocketDebuggerUrl})`);

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

  await new Promise((r) => setTimeout(r, 1000));

  // 1. Capture Main Queue Dashboard
  console.log('Capturing View 1: 01_queue_dashboard.png');
  await capture('01_queue_dashboard.png');

  // 2. Open Batch URLs modal
  console.log('Opening Batch URLs modal...');
  await evaluate(`
    (() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const batchBtn = buttons.find(b => b.textContent && b.textContent.includes('Batch URLs'));
      if (batchBtn) batchBtn.click();
    })()
  `);
  await new Promise((r) => setTimeout(r, 600));
  console.log('Capturing View 2: 02_batch_modal.png');
  await capture('02_batch_modal.png');

  // Close Batch modal
  await evaluate(`
    (() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const cancelBtn = buttons.find(b => b.textContent && (b.textContent.includes('Cancel') || b.textContent.includes('Close')));
      if (cancelBtn) cancelBtn.click();
    })()
  `);
  await new Promise((r) => setTimeout(r, 500));

  // 3. Switch to History tab
  console.log('Switching to History Tab...');
  await evaluate(`
    (() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const histBtn = buttons.find(b => b.textContent && b.textContent.includes('History'));
      if (histBtn) histBtn.click();
    })()
  `);
  await new Promise((r) => setTimeout(r, 700));
  console.log('Capturing View 3: 03_history_analytics.png');
  await capture('03_history_analytics.png');

  // 4. Switch to Settings tab
  console.log('Switching to Settings Tab...');
  await evaluate(`
    (() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const setBtn = buttons.find(b => b.textContent && b.textContent.includes('Settings'));
      if (setBtn) setBtn.click();
    })()
  `);
  await new Promise((r) => setTimeout(r, 700));
  console.log('Capturing View 4: 04_settings_engine.png');
  await capture('04_settings_engine.png');

  // 5. Test FormatModal via sample direct link analysis
  console.log('Switching back to Queue and testing URL Analysis for FormatModal...');
  await evaluate(`
    (() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const qBtn = buttons.find(b => b.textContent && b.textContent.includes('Queue'));
      if (qBtn) qBtn.click();
    })()
  `);
  await new Promise((r) => setTimeout(r, 500));

  // Trigger parseUrl via React form or Tauri IPC
  const analyzeRes = await evaluate(`
    (async () => {
      const input = document.querySelector('input[type="text"]');
      if (input) {
        // Set sample video URL
        const sampleUrl = 'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4';
        const proto = Object.getPrototypeOf(input);
        const setVal = Object.getOwnPropertyDescriptor(proto, 'value').set;
        setVal.call(input, sampleUrl);
        input.dispatchEvent(new Event('input', { bubbles: true }));
        
        const form = input.closest('form');
        if (form) {
          form.dispatchEvent(new Event('submit', { cancelable: true, bubbles: true }));
          return 'Form submitted';
        }
      }
      return 'No input found';
    })()
  `);
  console.log('Analyze action result:', analyzeRes);

  // Wait for format modal to open from backend parse
  await new Promise((r) => setTimeout(r, 2500));
  console.log('Capturing View 5: 05_format_modal.png');
  await capture('05_format_modal.png');

  ws.close();
  console.log('Capture finished cleanly.');
}

main().catch((err) => {
  console.error('Capture failed:', err);
  process.exit(1);
});
