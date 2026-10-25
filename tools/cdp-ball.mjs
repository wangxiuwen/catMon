const port = process.argv[2] || '9223';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const ball = targets.find(t => (t.url || '').includes('ball.html'));
if (!ball) { console.log('NO BALL TARGET'); process.exit(1); }
const ws = new WebSocket(ball.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (m, p) => new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); } };
await new Promise(r => ws.onopen = r);
const ev = async expr => { const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true }); return r.exceptionDetails ? 'EXC: ' + (r.exceptionDetails.exception?.description || '').slice(0, 200) : JSON.stringify(r.result?.value); };
console.log('listen:', await ev(`(async()=>{try{const un=await window.__TAURI__.event.listen('ballprobe',()=>{});return 'OK';}catch(e){return 'ERR: '+(e&&e.message||e);}})()`));
console.log('snapshot:', await ev(`(async()=>{try{const s=await window.__TAURI__.core.invoke('get_snapshot');return 'cpu='+s.cpu.toFixed(1);}catch(e){return 'ERR: '+(e&&e.message||e);}})()`));
console.log('dom:', await ev(`document.getElementById('cpuV').textContent`));
process.exit(0);
