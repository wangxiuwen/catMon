// 对真实运行的面板截图并保存:node tools/cdp-shot.mjs [port] [outfile]
import fs from 'node:fs';
const port = process.argv[2] || '9223';
const out = process.argv[3] || 'docs/screenshot.png';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const main = targets.find(t => (t.title || '').includes('系统监视器'));
if (!main) { console.log('NO MAIN TARGET'); process.exit(1); }
const ws = new WebSocket(main.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (m, p) => new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); } };
await new Promise(r => ws.onopen = r);
const ev = async expr => (await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })).result?.value;
await ev(`window.__TAURI__.core.invoke('toggle_panel')`);
await new Promise(r => setTimeout(r, 2000));
const shot = await send('Page.captureScreenshot', { format: 'png' });
if (!shot || !shot.data) { console.log('NO SCREENSHOT'); process.exit(1); }
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('saved', out, fs.statSync(out).size, 'bytes');
await ev(`window.__TAURI__.core.invoke('toggle_panel')`);
process.exit(0);
