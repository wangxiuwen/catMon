// CDP 验证 CatMon 主面板事件链路：node tools/cdp-verify.mjs [port]
const port = process.argv[2] || '9223';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const main = targets.find(t => (t.title || '').includes('CatMon'));
if (!main) { console.log('NO MAIN TARGET; targets:', targets.map(t => t.title || t.url).join(' | ')); process.exit(1); }
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
console.log('targets:', targets.map(t => t.title || t.url).join(' | '));
console.log('snap-before-toggle:', await evalJs('window.__lastSnap ? window.__lastSnap.cpu : "NONE"'));
await evalJs(`window.__TAURI__.core.invoke('toggle_panel')`);
await new Promise(r => setTimeout(r, 1500));
const cpu1 = await evalJs('window.__lastSnap ? window.__lastSnap.cpu : "NONE"');
await new Promise(r => setTimeout(r, 2200));
const cpu2 = await evalJs('window.__lastSnap ? window.__lastSnap.cpu : "NONE"');
console.log('after-toggle cpu:', cpu1, '-> 2s later:', cpu2);
console.log('VERDICT:', (cpu1 !== 'NONE' && cpu2 !== 'NONE' && cpu1 !== cpu2) ? 'EVENTS FLOWING' : 'STILL BROKEN');
console.log('hint:', await evalJs(`document.getElementById('metricHint').textContent`));
console.log('sysinfo groups:', await evalJs(`document.querySelectorAll('#sysinfo .sig').length`));
await evalJs(`window.__TAURI__.core.invoke('toggle_panel')`);
process.exit(0);
