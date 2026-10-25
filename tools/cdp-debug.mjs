// 深查主页面状态：__TAURI__ 是否注入、invoke 是否可用、控制台/异常
const port = process.argv[2] || '9223';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const main = targets.find(t => (t.title || '').includes('CatMon'));
const ws = new WebSocket(main.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const logs = [];
function send(method, params) {
  return new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
}
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); }
  else if (m.method === 'Runtime.consoleAPICalled') {
    logs.push(m.params.type + ': ' + (m.params.args || []).map(a => a.value ?? a.description ?? '').join(' '));
  } else if (m.method === 'Runtime.exceptionThrown') {
    logs.push('EXC: ' + JSON.stringify(m.params.exceptionDetails?.exception?.description || m.params.exceptionDetails?.text || '').slice(0, 400));
  }
};
await new Promise(r => ws.onopen = r);
await send('Runtime.enable', {});
async function evalJs(expr) {
  const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
  return { value: r && r.result ? r.result.value : undefined, err: r && r.exceptionDetails ? (r.exceptionDetails.exception?.description || r.exceptionDetails.text).slice(0, 300) : null };
}
console.log('T type:', (await evalJs('typeof window.__TAURI__')).value);
console.log('T keys:', (await evalJs('window.__TAURI__ ? Object.keys(window.__TAURI__).join(",") : null')).value);
console.log('core keys:', (await evalJs('window.__TAURI__ && window.__TAURI__.core ? Object.keys(window.__TAURI__.core).join(",") : null')).value);
console.log('readyState:', (await evalJs('document.readyState')).value);
console.log('lastSnap:', (await evalJs('typeof window.__lastSnap')).value);
const inv = await evalJs('(async () => { try { const c = await window.__TAURI__.core.invoke("get_config"); return JSON.stringify(c); } catch (e) { return "INVOKE-ERR: " + e; } })()');
console.log('invoke get_config:', typeof inv === 'string' ? inv.slice(0, 200) : inv);
await new Promise(r => setTimeout(r, 800));
console.log('--- console/exceptions ---');
logs.slice(-10).forEach(l => console.log(l.slice(0, 300)));
process.exit(0);
