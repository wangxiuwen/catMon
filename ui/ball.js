// 桌面悬浮球：随 metrics 事件刷新 CPU 环 + 内存；左键开关面板，拖动移动，右键隐藏。
const T = window.__TAURI__;
const invoke = T.core.invoke;
const ring = document.getElementById('ring');
const cpuV = document.getElementById('cpuV');
const memV = document.getElementById('memV');

function apply(s) {
  const cpu = Math.max(0, Math.min(100, s.cpu || 0));
  const mem = s.mem_total ? (s.mem_used / s.mem_total) * 100 : 0;
  ring.style.setProperty('--p', cpu.toFixed(0));
  cpuV.textContent = cpu.toFixed(0) + '%';
  memV.textContent = 'M ' + mem.toFixed(0) + '%';
}

// 左键点击（未拖动）开/关面板；拖动交给 startDragging；300ms 防抖防连点闪烁
let down = null;
let dragging = false;
let lastToggle = 0;
const ball = document.getElementById('ball');
ball.addEventListener('mousedown', e => {
  if (e.button !== 0) return;
  down = [e.screenX, e.screenY];
  dragging = false;
});
window.addEventListener('mousemove', e => {
  if (!down || dragging) return;
  if (Math.hypot(e.screenX - down[0], e.screenY - down[1]) > 6) {
    dragging = true;
    T.window.Window.getCurrent().startDragging();
  }
});
window.addEventListener('mouseup', () => {
  if (down && !dragging) {
    const now = Date.now();
    if (now - lastToggle > 300) {
      lastToggle = now;
      invoke('toggle_panel').catch(console.error);
    }
  }
  down = null;
  dragging = false;
});
ball.addEventListener('contextmenu', e => {
  e.preventDefault();
  invoke('hide_ball').catch(console.error);
});

(async () => {
  await T.event.listen('metrics', e => apply(e.payload));
  const s = await T.core.invoke('get_snapshot');
  if (s) apply(s);
})();
