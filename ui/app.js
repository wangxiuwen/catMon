// CatMon 监控面板。在 Tauri 里走真实数据；直接用浏览器打开则是演示模式。
const T = window.__TAURI__ || null;
const HIST = 120;
const S = { cpu: [], mem: [], rx: [], tx: [], snap: null, mode: 'cpu', checkedIds: null };
const $ = id => document.getElementById(id);

function fmtRate(b) {
  b = b || 0;
  if (b >= 1073741824) return (b / 1073741824).toFixed(2) + ' GB/s';
  if (b >= 1048576) return (b / 1048576).toFixed(1) + ' MB/s';
  if (b >= 1024) return (b / 1024).toFixed(1) + ' KB/s';
  return Math.round(b) + ' B/s';
}
function fmtSize(b) {
  if (b >= 1073741824) return (b / 1073741824).toFixed(1) + ' GB';
  if (b >= 1048576) return (b / 1048576).toFixed(0) + ' MB';
  return (b / 1024).toFixed(0) + ' KB';
}
function fmtDur(sec) {
  const h = Math.floor(sec / 3600), m = Math.floor(sec % 3600 / 60);
  return h > 0 ? h + ' 小时 ' + m + ' 分' : m + ' 分钟';
}
function esc(s) {
  return String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}
function push(a, v) { a.push(v); if (a.length > HIST) a.shift(); }

function prep(cv) {
  const dpr = devicePixelRatio || 1;
  const w = cv.clientWidth, h = cv.clientHeight;
  if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
    cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
  }
  const g = cv.getContext('2d');
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, w, h);
  return [g, w, h];
}
function grid(g, w, h) {
  g.strokeStyle = 'rgba(255,255,255,.06)';
  g.beginPath();
  for (let i = 1; i < 4; i++) { const y = h * i / 4; g.moveTo(0, y); g.lineTo(w, y); }
  g.stroke();
}
function series(g, data, max, color, w, h) {
  if (!data.length) return;
  g.beginPath();
  data.forEach((v, i) => {
    const x = i / (HIST - 1) * w;
    const y = h - 2 - (Math.max(0, Math.min(max, v)) / max) * (h - 6);
    i ? g.lineTo(x, y) : g.moveTo(x, y);
  });
  g.strokeStyle = color; g.lineWidth = 1.6; g.stroke();
  g.lineTo((data.length - 1) / (HIST - 1) * w, h);
  g.lineTo(0, h); g.closePath();
  g.globalAlpha = .14; g.fillStyle = color; g.fill(); g.globalAlpha = 1;
}
function drawSpark() {
  const [g, w, h] = prep($('cpuSpark'));
  series(g, S.cpu, 100, '#53d3ff', w, h);
}
function drawMain() {
  const [g, w, h] = prep($('chartMain'));
  grid(g, w, h);
  series(g, S.cpu, 100, '#53d3ff', w, h);
  series(g, S.mem, 100, '#a78bfa', w, h);
}
function drawNet() {
  const [g, w, h] = prep($('chartNet'));
  grid(g, w, h);
  const max = Math.max(1e5, ...S.rx, ...S.tx) * 1.15;
  series(g, S.rx, max, '#6ee7a0', w, h);
  series(g, S.tx, max, '#f59e0b', w, h);
}

let procCounter = 0;
function renderProcs(force) {
  const s = S.snap; if (!s) return;
  procCounter++;
  // 进程表每秒 innerHTML 全量重建有布局抖动，非强制时每 3 秒刷一次
  if (!force && procCounter % 3 !== 1) return;
  const list = S.mode === 'cpu' ? (s.procs_cpu || []) : (s.procs_mem || []);
  $('procBody').innerHTML = list.map(p =>
    `<tr><td>${esc(p.name)}</td><td>${p.pid}</td><td>${(p.cpu || 0).toFixed(1)}%</td><td>${fmtSize(p.mem)}</td></tr>`
  ).join('') || '<tr><td colspan="4">--</td></tr>';
}

let lastApplyAt = 0;
function apply(s) {
  window.__lastSnap = s; // 调试钩子：CDP 可验证事件链路
  // 面板隐藏期间事件停发，重开后清空历史，避免曲线把时间断档画成假连线
  const now = Date.now();
  if (lastApplyAt && now - lastApplyAt > 10000) {
    S.cpu.length = S.mem.length = S.rx.length = S.tx.length = 0;
  }
  lastApplyAt = now;
  S.snap = s;
  push(S.cpu, s.cpu || 0);
  push(S.mem, s.mem_total ? s.mem_used / s.mem_total * 100 : 0);
  push(S.rx, s.rx || 0);
  push(S.tx, s.tx || 0);

  $('cpuV').textContent = (s.cpu || 0).toFixed(0) + '%';
  // parked 核会报 0，最小值只看有读数的核
  const mz = (s.core_mhz || []).filter(v => v > 0);
  $('cpuDetail').textContent = mz.length
    ? (Math.min(...mz) / 1000).toFixed(2) + ' – ' + (Math.max(...mz) / 1000).toFixed(2) + ' GHz'
    : '';
  $('memV').textContent = fmtSize(s.mem_used) + ' / ' + fmtSize(s.mem_total);
  $('memDetail').textContent = (s.swap_total ? `Swap ${fmtSize(s.swap_used)} / ${fmtSize(s.swap_total)}` : '');
  $('memBar').style.width = (s.mem_total ? s.mem_used / s.mem_total * 100 : 0) + '%';
  $('rxV').textContent = fmtRate(s.rx);
  $('txV').textContent = fmtRate(s.tx);

  const box = $('cores');
  if (s.cores && box.childElementCount !== s.cores.length) {
    box.innerHTML = s.cores.map(() => '<div class="core"><i></i></div>').join('');
  }
  if (s.cores) [...box.children].forEach((c, i) => {
    const v = Math.max(0, Math.min(100, s.cores[i] || 0));
    const bar = c.firstElementChild;
    bar.style.height = v + '%';
    bar.style.background = v > 85 ? '#f87171' : v > 60 ? '#fbbf24' : '#53d3ff';
  });

  drawSpark(); drawMain(); drawNet(); renderProcs();

  const dl = $('disks');
  dl.innerHTML = (s.disks || []).map(d => {
    const used = d.total - d.avail, p = d.total ? used / d.total * 100 : 0;
    return `<div class="disk"><div class="dl"><span>${esc(d.mount)}</span><span>${fmtSize(used)} / ${fmtSize(d.total)}</span></div>` +
      `<div class="bar"><i style="width:${p}%;background:${p > 90 ? '#f87171' : '#53d3ff'}"></i></div></div>`;
  }).join('') || '<div class="detail">--</div>';
  const main = (s.disks || []).slice().sort((a, b) => b.total - a.total)[0];
  if (main) {
    const p = main.total ? (main.total - main.avail) / main.total * 100 : 0;
    $('diskV').textContent = p.toFixed(0) + '%';
    $('diskDetail').textContent = fmtSize(main.total - main.avail) + ' / ' + fmtSize(main.total);
    $('diskBar').style.width = p + '%';
  }

  $('temps').textContent = (s.temps && s.temps.length)
    ? s.temps.map(t => `${t.label} ${t.celsius.toFixed(0)}°C`).join(' · ')
    : '系统未提供';

  $('foot').textContent = `已运行 ${fmtDur(s.uptime || 0)} · 本次开机累计 ↓${fmtSize(s.rx_total || 0)} ↑${fmtSize(s.tx_total || 0)}` +
    (T ? '' : ' · 浏览器演示模式（模拟数据）');
}

// ---- 清理页 ----
function fmtB(b) { return b >= 1073741824 ? (b / 1073741824).toFixed(2) + ' GB' : b >= 1048576 ? (b / 1048576).toFixed(1) + ' MB' : b >= 1024 ? (b / 1024).toFixed(0) + ' KB' : b + ' B'; }

function catRow(c) {
  // 用户清理过一次后按其勾选记忆恢复；首次进默认全选（有内容的项）
  const keep = Array.isArray(S.checkedIds);
  const on = keep ? S.checkedIds.includes(c.id) : c.count > 0;
  return `
    <label class="cat ${c.count ? '' : 'zero'}">
      <input type="checkbox" data-id="${esc(c.id)}" ${on && c.count ? 'checked' : 'disabled'}>
      <div style="min-width:0">
        <div class="ct">${esc(c.title)}</div>
        <div class="cd" title="${esc(c.detail)}">${esc(c.detail)}</div>
      </div>
      <div class="cs">${c.count} 项${c.bytes ? ' · ' + fmtB(c.bytes) : ''}</div>
    </label>`;
}

function renderClean(scan) {
  const files = scan.categories.filter(c => c.group === 'file');
  const regs = scan.categories.filter(c => c.group === 'reg');
  $('cleanFiles').innerHTML = files.map(catRow).join('') || '<span class="detail">没有发现</span>';
  $('cleanReg').innerHTML = regs.map(catRow).join('') || '<span class="detail">没有发现</span>';
}

// 浏览器演示模式的启动项（模块级，切换 disabled 后重渲染才能看到状态翻转）
const DEMO_STARTUP = [
  { id: 'hkcu-run:DEMO', source: '注册表 HKCU\\Run', name: 'DemoStartup', cmd: '"C:\\demo\\start.exe" /min', admin: false, disabled: false },
  { id: 'hkcu-run:DEMO2', source: '注册表 HKCU\\Run', name: 'DemoPaused', cmd: '"C:\\demo\\off.exe" /quiet', admin: false, disabled: true },
  { id: 'hklm-run:DEMO3', source: '注册表 HKLM\\Run', name: 'DemoSys', cmd: 'C:\\demo\\sys.exe', admin: true },
];

async function loadStartup() {
  $('startupBody').innerHTML = '<tr><td colspan="3" class="detail">扫描中…</td></tr>';
  const list = T ? await T.core.invoke('startup_scan') : DEMO_STARTUP;
  $('startupBody').innerHTML = list.length ? list.map(s => `
    <tr><td>${esc(s.name)}${s.disabled ? ' <span class="detail">（已禁用）</span>' : ''}<div class="detail">${esc(s.source)}</div></td>
    <td class="detail" style="max-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(s.cmd)}</td>
    <td style="white-space:nowrap">${s.admin ? '<span class="detail">需管理员</span>' : `<button class="rowbtn" data-toggle="${esc(s.id)}" data-dis="${s.disabled ? 1 : 0}">${s.disabled ? '启用' : '禁用'}</button> <button class="rowbtn" data-del="${esc(s.id)}">删除</button>`}</td></tr>`).join('')
    : '<tr><td colspan="3" class="detail">没有启动项</td></tr>';
  $('startupBody').querySelectorAll('button[data-del]').forEach(b => b.onclick = async () => {
    b.disabled = true; b.textContent = '…';
    const rep = T ? await T.core.invoke('startup_remove', { id: b.dataset.del }) : { removed_files: 1, backups: ['备份 x1'] };
    $('cleanReport').textContent = `已删除启动项（备份 ${rep.backups.length} 份）` + (rep.errors.length ? ' · ' + rep.errors.join(';') : '');
    loadStartup();
  });
  $('startupBody').querySelectorAll('button[data-toggle]').forEach(b => b.onclick = async () => {
    b.disabled = true;
    try {
      // 演示模式按渲染时的状态取反（真实模式以后端返回为准）
      const nowDisabled = T
        ? await T.core.invoke('startup_toggle', { id: b.dataset.toggle })
        : b.dataset.dis !== '1';
      if (!T) {
        const item = DEMO_STARTUP.find(s => s.id === b.dataset.toggle);
        if (item) item.disabled = nowDisabled;
      }
      $('cleanReport').textContent = nowDisabled ? '已禁用该启动项' : '已启用该启动项';
    } catch (e) {
      $('cleanReport').textContent = '切换失败: ' + e;
    }
    loadStartup();
  });
}

let scanning = false;
async function rescan() {
  if (scanning) return; // 扫描中（导航切换会误触发）直接忽略
  scanning = true;
  $('btnRescan').disabled = true;
  $('cleanFiles').innerHTML = '<span class="detail">扫描中…</span>';
  $('cleanReg').innerHTML = '';
  $('cleanReport').textContent = '';
  try {
    if (T) {
      const c = await T.core.invoke('get_config');
      const lc = c.last_clean;
      $('lastClean').textContent = lc
        ? `上次清理 ${new Date(lc.ts * 1000).toLocaleString()} · 释放 ${fmtB(lc.freed)}`
        : '尚未清理过';
    }
    const scan = T ? await T.core.invoke('clean_scan') : {
    categories: [
      { id: 'temp', group: 'file', title: '用户临时文件', detail: 'Temp（超过 24 小时）', count: 3841, bytes: 2.4e9 },
      { id: 'browser', group: 'file', title: '浏览器缓存', detail: 'Chrome / Edge / Firefox', count: 8123, bytes: 3.1e9 },
      { id: 'devcache', group: 'file', title: '开发工具缓存', detail: 'pip · npm · cargo', count: 912, bytes: 5.7e9 },
      { id: 'recyclebin', group: 'file', title: '回收站', detail: '清空所有分区', count: 12, bytes: 8.6e8 },
      { id: 'muicache', group: 'reg', title: '注册表：图标/名称缓存残留', detail: 'MuiCache', count: 23, bytes: 0 },
      { id: 'uninstall', group: 'reg', title: '注册表：卸载残留', detail: '卸载程序已丢失的条目', count: 0, bytes: 0 },
      { id: 'privacy', group: 'reg', title: '注册表：隐私痕迹', detail: '最近文档 · 运行框历史', count: 156, bytes: 0 },
    ],
  };
  renderClean(scan);
  } catch (e) {
    console.error(e);
    $('cleanFiles').innerHTML = '<span class="detail">扫描失败，请重试</span>';
  }
  $('btnRescan').disabled = false;
}

async function doClean() {
  const ids = [...document.querySelectorAll('#viewClean input:checked')].map(i => i.dataset.id);
  if (!ids.length) { $('cleanReport').textContent = '没有勾选任何项'; return; }
  S.checkedIds = ids; // 记住用户的选择，重扫后按此恢复勾选
  const btn = $('btnClean');
  btn.disabled = true; btn.textContent = '清理中…';
  let rep;
  try {
    rep = T ? await T.core.invoke('clean_execute', { ids }) : { freed_bytes: 3.1e9, removed_files: 3820, removed_reg_items: 26, backups: ['备份 x3'], errors: [] };
  } catch (e) {
    btn.disabled = false; btn.textContent = '清理所选';
    $('cleanReport').textContent = '清理执行失败: ' + e;
    return;
  }
  btn.disabled = false; btn.textContent = '清理所选';
  await rescan(); loadStartup();
  // 报告写在 rescan 之后，避免被 rescan 的清空逻辑吃掉
  $('cleanReport').textContent =
    `已释放 ${fmtB(rep.freed_bytes)} · 删除文件 ${rep.removed_files} 个 · 注册表清理 ${rep.removed_reg_items} 项` +
    (rep.backups && rep.backups.length ? ` · 注册表备份 ${rep.backups.length} 份（.reg，双击可还原）` : '') +
    (rep.errors && rep.errors.length ? ` · 失败：${rep.errors.length} 条` : '');
}

async function scanBig() {
  const btn = $('btnBig'); btn.disabled = true;
  $('bigBody').innerHTML = '<tr><td colspan="3" class="detail">扫描中…（桌面/下载/文档/视频/图片）</td></tr>';
  const list = T ? await T.core.invoke('large_files_scan') : [
    { path: 'D:\\demo\\虚拟机\\ubuntu.qcow2', size: 9.3e9, mtime: Date.now() / 1000 - 86400 * 30 },
    { path: 'C:\\Users\\DEMO\\Downloads\\setup.iso', size: 5.2e9, mtime: Date.now() / 1000 - 86400 * 3 },
  ];
  S.bigList = list;
  renderBig();
  btn.disabled = false;
}

let bigMin = 100 * 1024 * 1024;
function renderBig() {
  const list = (S.bigList || []).filter(f => f.size >= bigMin);
  $('bigBody').innerHTML = list.length ? list.map(f => `
    <tr><td title="${esc(f.path)}" style="max-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(f.path)}</td>
    <td>${fmtB(f.size)}</td><td class="detail">${new Date(f.mtime * 1000).toLocaleDateString()}</td></tr>`).join('')
    : `<tr><td colspan="3" class="detail">该阈值下没有大文件</td></tr>`;
}

function switchView(v) {
  $('viewMon').style.display = v === 'mon' ? '' : 'none';
  $('viewClean').style.display = v === 'clean' ? '' : 'none';
  $('navMon').classList.toggle('on', v === 'mon');
  $('navClean').classList.toggle('on', v === 'clean');
}

function renderInfo(info) {
  window.__lastInfo = info;
  const box = $('sysinfo');
  box.innerHTML = (info.groups || []).map(g =>
    `<div class="sig"><div class="sig-t">${esc(g.title)}</div>` +
    g.items.map(it =>
      `<div class="kv"><span class="k2">${esc(it.k)}</span><span class="v2">${esc(it.v)}</span></div>`
    ).join('') + `</div>`
  ).join('') || '<span class="detail">未采集到</span>';
}

async function refreshHint() {
  const cfg = await T.core.invoke('get_config');
  const chars = await T.core.invoke('list_characters');
  const names = { cpu: 'CPU', mem: '内存', net: '网络' };
  const cur = (chars.find(c => c[2]) || ['', ''])[1];
  $('metricHint').textContent =
    `${cur ? '角色：' + cur + ' · ' : ''}速度跟随 ${names[cfg.metric] || cfg.metric}${cfg.paused ? '（已暂停）' : ''}${cfg.autostart ? ' · 已开机自启' : ''}`;
}

async function init() {
  if (T) {
    // 先挂监听再取快照，任何一步失败都不影响事件流
    await T.event.listen('metrics', e => apply(e.payload));
    await T.event.listen('config', () => refreshHint().catch(console.error));
    await T.event.listen('sysinfo', e => renderInfo(e.payload));
    try { T.app.getVersion().then(v => { $('ver').textContent = 'CatMon v' + v; }); } catch (e) {}
    await T.event.listen('clean-progress', e => {
      const p = e.payload || {};
      if (p.done) { $('btnClean').textContent = '清理所选'; return; }
      if (typeof p.i === 'number') $('btnClean').textContent = `清理中 ${p.i + 1}/${p.total}`;
    });
    try {
      const s = await T.core.invoke('get_snapshot');
      if (s) apply(s);
    } catch (e) { console.error(e); }
    try {
      const si = await T.core.invoke('get_system_info');
      if (si) renderInfo(si);
    } catch (e) { console.error(e); }
    refreshHint().catch(console.error);
  } else {
    demo();
  }
}

// ---- 浏览器演示模式 ----
function demo() {
  renderInfo({ groups: [
    { title: '操作系统', items: [
      { k: '系统', v: 'Windows 11 专业工作站版' }, { k: '版本', v: '10.0.26300' },
      { k: '内核', v: '10.0.26300' }, { k: '主机名', v: 'DEMO-PC' }, { k: '架构', v: 'x86_64' }] },
    { title: '处理器', items: [
      { k: '型号', v: 'AMD Ryzen 9 7950X 16-Core Processor' }, { k: '核心', v: '物理 16 / 逻辑 32' },
      { k: '插槽', v: 'AM5' }, { k: '基准频率', v: '4500 MHz' },
      { k: '厂商', v: 'AMD' }, { k: '家族/型号/步进', v: '19 / 61 / 2' },
      { k: '指令集', v: 'SSE / SSE4.2 / AVX / FMA3 / AES-NI / AVX2 / SHA' },
      { k: 'L1 缓存', v: '1.0 MB（每核）' }, { k: 'L2 缓存', v: '16.0 MB（每核）' }, { k: 'L3 缓存', v: '64.0 MB（共享）' }] },
    { title: '内存', items: [
      { k: '物理内存', v: '32.0 GB' }, { k: 'Swap', v: '8.0 GB' }] },
    { title: '内存（插槽）', items: [
      { k: '条1', v: 'Controller0-ChannelA-DIMM0 · 16.0 GB · DDR5-5600 · Kingston · KF556C40-16' },
      { k: '条2', v: 'Controller1-ChannelA-DIMM0 · 16.0 GB · DDR5-5600 · Kingston · KF556C40-16' }] },
    { title: '显卡', items: [
      { k: '显卡', v: 'NVIDIA GeForce RTX 4070' }, { k: '驱动', v: '32.0.15.6109' }, { k: '分辨率', v: '2560×1440' }] },
    { title: '主板 / BIOS', items: [
      { k: '主板', v: 'ASUSTeK ROG STRIX X670E-E' }, { k: 'BIOS', v: 'American Megatrends 2.08' }, { k: 'BIOS 日期', v: '2023-08-01' }] },
  ] });
  const fakeProcs = [
    { name: 'chrome.exe', pid: 4242, cpu: 18.4, mem: 2.1e9 },
    { name: 'Code.exe', pid: 812, cpu: 9.7, mem: 1.6e9 },
    { name: 'catmon.exe', pid: 7777, cpu: 1.2, mem: 24e6 },
    { name: 'explorer.exe', pid: 1024, cpu: 0.8, mem: 180e6 },
    { name: 'WeChat.exe', pid: 3389, cpu: 2.3, mem: 640e6 },
  ];
  let t = 0;
  setInterval(() => {
    t++;
    const cpu = Math.max(2, Math.min(100,
      25 + 45 * Math.abs(Math.sin(t / 17)) + 18 * Math.abs(Math.sin(t / 3.7)) + (Math.random() * 6 - 3)));
    apply({
      cpu,
      cores: Array.from({ length: 16 }, () => Math.max(0, Math.min(100, cpu + (Math.random() * 30 - 15)))),
      core_mhz: Array.from({ length: 16 }, () => Math.round(4100 + Math.random() * 1500)),
      mem_used: 19e9 + 6e9 * Math.abs(Math.sin(t / 60)) + (Math.random() - .5) * 2e8,
      mem_total: 34359738368, swap_used: 2e9, swap_total: 8e9,
      rx: Math.abs(Math.sin(t / 6)) * 3.5e6 * Math.random() + Math.random() * 40000,
      tx: Math.abs(Math.sin(t / 8 + 1)) * 8e5 + Math.random() * 20000,
      rx_total: t * 2.4e6, tx_total: t * 4e5,
      disks: [
        { mount: 'C:/', total: 476e9, avail: 120e9 },
        { mount: 'D:/', total: 1024e9, avail: 640e9 },
      ],
      procs_cpu: fakeProcs, procs_mem: fakeProcs.slice().reverse(),
      temps: [{ label: 'CPU', celsius: 58 + Math.random() * 8 }, { label: 'GPU', celsius: 47 + Math.random() * 5 }],
      uptime: t + 8 * 3600,
    });
  }, 1000);
}

$('tabCpu').onclick = () => { S.mode = 'cpu'; $('tabCpu').classList.add('on'); $('tabMem').classList.remove('on'); renderProcs(true); };
$('tabMem').onclick = () => { S.mode = 'mem'; $('tabMem').classList.add('on'); $('tabCpu').classList.remove('on'); renderProcs(true); };
$('navMon').onclick = () => switchView('mon');
$('navClean').onclick = () => {
  switchView('clean');
  if (!$('cleanFiles').querySelector('.cat')) { rescan(); loadStartup(); }
};
$('btnRescan').onclick = rescan;
$('btnClean').onclick = doClean;
$('btnBig').onclick = scanBig;
document.querySelectorAll('#bigThresh button').forEach(b => b.onclick = () => {
  bigMin = Number(b.dataset.mb) * 1024 * 1024;
  document.querySelectorAll('#bigThresh button').forEach(x => x.classList.toggle('on', x === b));
  if (S.bigList) renderBig();
});
$('btnCopyInfo').onclick = async () => {
  const info = window.__lastInfo;
  if (!info) return;
  const text = (info.groups || []).map(g =>
    `【${g.title}】\n` + g.items.map(it => `${it.k}：${it.v}`).join('\n')
  ).join('\n\n');
  try {
    await navigator.clipboard.writeText(text);
  } catch (e) {
    // 剪贴板 API 不可用（如文档失焦）时的兜底
    const ta = document.createElement('textarea');
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand('copy');
    ta.remove();
  }
  $('copyMsg').textContent = '已复制 ✓';
  setTimeout(() => { $('copyMsg').textContent = ''; }, 2000);
};
$('btnDns').onclick = async () => {
  const ok = T ? await T.core.invoke('flush_dns') : true;
  $('dnsMsg').textContent = ok ? '已刷新 ✓' : '刷新失败';
};
init();
