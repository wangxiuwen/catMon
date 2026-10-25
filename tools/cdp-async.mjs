// 验证扫描期间 UI 不冻结：发起 clean_scan（秒级）的同时，页面应仍能快速响应 evaluate
const port = process.argv[2] || '9223';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const main = targets.find(t => (t.title || '').includes('CatMon'));
const ws = new WebSocket(main.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
function send(method, params) {
  return new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
}
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); }
};
await new Promise(r => ws.onopen = r);
async function evalJs(expr) {
  const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
  return r && r.result ? r.result.value : undefined;
}
await evalJs(`window.__scanN = null; window.__TAURI__.core.invoke('clean_scan').then(r => { window.__scanN = (r.categories || []).length; }).catch(e => { window.__scanN = 'ERR:' + e; }); 'scan started'`);
const samples = [];
for (let i = 0; i < 10; i++) {
  const t0 = Date.now();
  await evalJs('1+1');
  samples.push(Date.now() - t0);
  await new Promise(r => setTimeout(r, 300));
}
const worst = Math.max(...samples);
let n = null;
for (let i = 0; i < 40 && n === null; i++) {
  await new Promise(r => setTimeout(r, 500));
  n = await evalJs('window.__scanN');
}
console.log('ui response samples(ms):', samples.join(','));
console.log('worst:', worst, 'ms ->', worst < 500 ? 'UI RESPONSIVE DURING SCAN' : 'UI FROZE');
console.log('scan categories:', n);
process.exit(0);
