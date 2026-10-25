import io

p = "ui/app.js"
s = io.open(p, encoding="utf-8").read()

old = """  btn.disabled = true; btn.textContent = '清理中…';
  const rep = T ? await T.core.invoke('clean_execute', { ids }) : { freed_bytes: 3.1e9, removed_files: 3820, removed_reg_items: 26, backups: ['备份 x3'], errors: [] };
  $('cleanReport').textContent ="""
new = """  btn.disabled = true; btn.textContent = '清理中…';
  let rep;
  try {
    rep = T ? await T.core.invoke('clean_execute', { ids }) : { freed_bytes: 3.1e9, removed_files: 3820, removed_reg_items: 26, backups: ['备份 x3'], errors: [] };
  } catch (e) {
    btn.disabled = false; btn.textContent = '清理所选';
    $('cleanReport').textContent = '清理执行失败: ' + e;
    return;
  }
  $('cleanReport').textContent ="""

assert old in s, "anchor missing"
s = s.replace(old, new)
io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("patched doClean try/catch")
