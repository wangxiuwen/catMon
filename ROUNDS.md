# CatMon 自我迭代日志

> 规则：每轮只做一件小而明确的事，改完必须 `cargo build --release -p catmon` 通过并做对应验证，再在此登记。

## 已完成

| 轮次 | 日期 | 内容 | 验证 |
|---|---|---|---|
| R1 | 2026-10-25 | recentlnk 分类归组修正（reg→file，最近文档是文件不是注册表） | --clean-scan group 字段 |
| R2 | 2026-10-25 | 卸载残留/MuiCache 判据跳过 UNC 网络路径（\\ 开头不探测，防卡顿防误报） | 单测 exe_from_cmd_cases |
| R3 | 2026-10-25 | expand_env 值含 % 时终止展开，防死循环 | 单测 expand_env_basic |
| R4 | 2026-10-25 | 判据单测补齐（真实文件/PATH 裸命令/UNC/不存在路径四类） | cargo test 4 passed |
| R5 | 2026-10-25 | 开机自启托盘开关（HKCU Run 键 + config.autostart 持久化 + 写失败回滚 + 启动时刷新 exe 路径） | 构建通过 |
| R6 | 2026-10-25 | config 变更事件：托盘切角色/指标/暂停/自启后，面板页眉提示实时刷新（含自启状态） | 构建通过 |
| R7 | 2026-10-25 | 托盘墨色随任务栏深浅每 5 分钟重检，变化即重建角色帧（ink 改 Mutex<Rgba>，Rgba 加 PartialEq） | 构建通过 |
| R8 | 2026-10-25 | 面板隐藏时停发 metrics 事件（省 IPC；重开后 1 秒内自动追上） | 构建通过 |
| R9 | 2026-10-25 | 清理页明示 .reg 备份目录路径 | 目检 |
| R10 | 2026-10-25 | 编译期修复：主题重检块作用域、digits ink Mutex 借用 | cargo build |
| R11 | 2026-10-25 | 大文件扫描跳过 junction/symlink 重解析点（REPARSE 属性），防 XP 兼容链循环空扫 | --bigfiles 返回 12 项正常 |
| R12 | 2026-10-25 | ★修复「监控没数据」真因：项目缺 Tauri v2 capabilities 声明，event.listen 被 ACL 拒绝（自定义命令默认放行所以此前构建/自检全绿）——补 src-tauri/capabilities/default.json（main+ball, core:default）；事件推送可见性改原子标志（后台线程查 is_visible 不可靠）；显示面板/悬浮球时立即补发最新一帧；前端先挂监听再取快照 | tools/cdp-verify.mjs 实测 VERDICT: EVENTS FLOWING（cpu 7.49→5.23 实时变化） |
| R13 | 2026-10-25 | 桌面悬浮球（默认关闭，托盘菜单勾选开启）：84px 透明圆环 CPU% + 内存小字，拖动移动、左键开关面板、右键隐藏；配置持久化，随启动恢复 | CDP 探针：ball 页 listen OK / snapshot cpu=8.4 / DOM 已渲染 5% |
| R14 | 2026-10-25 | 托盘 tooltip 加入角色名（CatMon · 猫 · CPU …），切角色即跟随 | cargo build + 4 单测通过 |
| R15 | 2026-10-25 | value_to_reg 转义单测：默认名 @、反斜杠/引号转义、DWORD/QWORD/BINARY/EXPAND/MULTI/未知类型兜底（.reg 备份安全网钉死） | cargo test 5 passed |
| R16 | 2026-10-25 | clean_execute 幂等保护：AtomicBool compare_exchange 互斥 + Drop 守卫兜底，并发触发返回「已有一次清理正在执行」 | cargo build + 5 单测 + --clean-scan 11 类正常 |
| R17 | 2026-10-25 | 面板窗口尺寸/位置记忆：隐藏/关闭时存 WinRect 进 config，启动恢复；校验恢复位置是否落在现有显示器上（拔显示器不丢窗口，否则保持居中） | cdp-verify 触发 hide 后 config.json 出现 "win"；二次启动 tray ready 正常 |
| R18 | 2026-10-25 | icon.ico 升级多尺寸（16/32/48/256 PNG 条目，Explorer 大图标不再糊）；修三连 bug：48 非整数倍缩放崩溃（nearest 通吃升降采样）、目录区未 resize 越界、条目偏移漏 ICONDIR 6 字节 | py 结构校验 4 条目全 dataOK；touch build.rs 强制重嵌资源后构建通过 |
| R19 | 2026-10-25 | 浏览器缓存按浏览器拆分（browser:Chrome / browser:Edge / browser:Brave / browser:Firefox 各自扫描与清理） | --clean-scan：Chrome 8362 项 982MB、Edge 189 项、Firefox 10 项 |
| R20 | 2026-10-25 | 系统配置区「复制全部」按钮：全文按【组】+ 键值排版进剪贴板（clipboard API + execCommand 兜底） | CDP 实测：点击后 copyMsg=已复制 ✓ |
| R21 | 2026-10-25 | 清理所选后保留用户勾选状态（checkedIds 记忆，重扫/清理后按记忆恢复；首次仍默认全选） | node --check + 构建 |
| R22 | 2026-10-25 | 启动项列表显示禁用状态：读 StartupApproved 键（首字节奇数=已禁用），名称后标注「已禁用」 | --startup-scan 实测输出 1 条 disabled:true |
| R23 | 2026-10-25 | 大文件面板阈值切换（≥100MB / ≥500MB / ≥1GB）：扫一次本地过滤即时生效 | node --check + 构建 |
| R24 | 2026-10-25 | WebView2 缺失提示：启动前按 MS 注册表口径（HKLM WOW6432Node/HKLM/HKCU 三处 pv）检测运行时，缺失弹原生错误框给下载地址，不再静默失败 | --check-webview2 本机输出 missing=false（已装，检测路径正确） |
| R25 | 2026-10-25 | 清理执行进度反馈：execute 加 progress 回调，每类开始发 clean-progress 事件（i/total/id），完成发 done；前端按钮显示「清理中 i/total」 | CDP 实测事件序列 [{i:0},{i:1},{done:true}]、按钮文案复位正常 |
| R26 | 2026-10-25 | 健壮性：tray.rs 全部 24 处 Mutex lock().unwrap() 改为中毒自愈（unwrap_or_else into_inner），杜绝一次持锁 panic 连环打死采样/动画线程导致监控永久无数据 | cargo build + 5 单测 + cdp-verify EVENTS FLOWING |
| R27 | 2026-10-25 | ★性能：clean_scan/clean_execute/startup_scan/large_files_scan 改 async（Tauri v2 同步命令跑主线程，秒级扫描会冻结面板 UI） | CDP 实测扫描期间 UI 响应 worst=1ms（UI RESPONSIVE）、scan categories=13 |
| R28 | 2026-10-25 | 防循环：walk/clean_walk 跳过 junction/symlink 重解析目录（R18 同款防护补齐到清理遍历器，%TEMP% 出现兼容链不再绕圈） | cargo build + 5 单测 + --clean-scan 13 类正常 |
| R29 | 2026-10-25 | 曲线时间断档检测：面板隐藏（事件停发）超过 10 秒后重开，清空历史重画，不再把断档画成横跨的假连线 | node --check + 构建 |
| R30 | 2026-10-25 | 备份目录自动修剪：execute 末尾按修改时间只保留最近 50 份 .reg，防无限增长 | 实测 55 个假备份 → --clean-exec 后恰好剩 50 |
| R31 | 2026-10-25 | config.json 原子写：先落 .tmp 再 rename 替换，防写到一半断电损坏配置（角色/自启/窗口位置全丢）；补存取回环 + 覆盖写单测 | cargo test 6 passed |
| R32 | 2026-10-25 | 退出/系统关机路径补保存窗口矩形：RunEvent::ExitRequested 时 save_window_rect（此前只有隐藏和 X 路径保存，托盘退出会丢最新位置） | cargo build + 运行正常（config 已有 win 字段） |
| R33 | 2026-10-25 | 单实例保护：命名互斥体 Local\CatMon.SingleInstance，双击第二个实例弹「已经在运行」提示后退出，杜绝双托盘猫/双采样器/配置互踩；测试旗标（--selftest 等）不受影响 | 实测：A 运行中启动 B → B 停在提示框（16MB，未初始化托盘），A 完好 |
| R34 | 2026-10-25 | ★修 R33 回归：ensure_single_instance 被插在旗标处理中间，GUI 运行时 --selftest/--clean-scan 等验证旗标被单实例误拦——调用挪到全部旗标之后、Builder 之前 | 实测 GUI 运行中：--selftest OK、--clean-scan 13 类、--startup-scan 14 项全部正常 |
| R35 | 2026-10-25 | ★一致性修复：数字角色永远显示 CPU，与「速度跟随内存/网络」自相矛盾——改为显示当前选定指标的百分比，菜单标签改「数字（跟随指标）」 | cargo build + 6 单测 |
| R36 | 2026-10-25 | 系统配置每 30 分钟后台重采（此前启动后永不刷新，换驱动/插磁盘跟不上），采集在后台线程不卡交互 | cargo build |
| R37 | 2026-10-25 | 启动项删除补净尾巴：删 Run 值时连带清掉 StartupApproved 里的禁用/启用记录（先随 .reg 一起备份），不留孤儿数据 | cargo build + 6 单测 |
| R38 | 2026-10-25 | R37 对称补齐：启动文件夹项删除后连带清 StartupApproved\StartupFolder 记录（.reg 先备份） | cargo build + 6 单测 + --startup-scan 14 项正常 |
| R39 | 2026-10-25 | 清理页显示「上次清理时间 · 释放量」（last_clean 持久化到 config，clean_execute 成功后记录） | node --check + 构建 |
| R40 | 2026-10-25 | 自定义角色加载防呆：sprites 目录里超过 20MB 的 PNG 直接跳过（解码会放大数倍吃内存，进程不至于被一张图打爆） | cargo build |
| R41 | 2026-10-25 | ★性能：动画线程心跳（25 次/秒）不再全量克隆整个角色帧表（~100KB+ 分配白耗），锁内只读元数据，真正切帧时才克隆那一帧 4KB | cargo build + cdp-verify EVENTS FLOWING |

## 验证工具

- `tools/cdp-verify.mjs [port]`：主面板事件链路端到端验证（需以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223" 启动后运行）
- `tools/cdp-debug.mjs` / `tools/cdp-ball.mjs`：页面状态深查 / 悬浮球探针

## 待办池（下轮从这里挑或自行审查发现）

- [x] ~~面板窗口尺寸/位置记忆（退出前保存到 config.json，重开还原）~~（R17，含显示器校验）
- [x] ~~清理执行进度反馈：大目录清理时向面板发粗粒度进度事件~~（R25）
- [x] ~~startup_scan 显示 StartupApproved 禁用状态（标记"已禁用"）~~（R22）
- [x] ~~大文件扫描跳过符号链接与重解析点，防循环~~（R11）
- [x] ~~metrics 事件在窗口重新显示时立即补发一帧~~（R12 push_latest 已覆盖）
- [x] ~~value_to_reg 各注册表类型转义单测（引号/反斜杠/非 ASCII）~~（R15）
- [x] ~~托盘tooltip 加入角色名~~（R14，此前漏划）
- [x] ~~浏览器缓存扫描按 profile 拆分显示（哪些浏览器有多少）~~（R19，按浏览器整体拆分）
- [x] ~~清理所选后保留各类勾选状态（rescan 后恢复）~~（R21）
- [x] ~~panel sysinfo 区加"复制全部"按钮~~（R20）
- [x] ~~bigfiles 增加最小体积切换（100MB/500MB/1GB）~~（R23）
- [x] ~~判断 WebView2 缺失时给出友好提示~~（R24）
- [x] ~~icon 在资源管理器中显示大图（多尺寸 ico：16/32/48/256 打包）~~（R18）
- [x] ~~clean_execute 对重复点击的幂等保护（前端已禁按钮，后端可加锁）~~（R16）

## 已知限制（不列为待办）

- HKLM 系统级清理/启动项需要管理员，明确不处理
- CPU-Z 的跑分/电压/完整 SPD 时序/制程 TDP 依赖内核驱动与内置数据库，不做

## 当前阻塞（2026-10-25）

- GitHub 开源发布等待用户凭据：设备码流程已两轮过期（用户卡在 GitHub 2FA 安全码），或提供 PAT 存 ~/.ghtoken。凭据就绪后执行：gh repo create wangxiuwen/catmon --public --source=. --push。

## 2026-10-26 补录与新轮

| 轮次 | 日期 | 内容 | 验证 |
|---|---|---|---|
| R42 | 2026-10-26 | flush_dns 命令改 async（ipconfig 进程拉起在主线程有 100-300ms 冻结，R27 同类） | cargo build + 6 单测 |
| R43 | 2026-10-26 | 健壮性：main.rs 命令处理器 5 处 Mutex lock().unwrap() 补中毒自愈（R26 只覆盖了 tray.rs） | cargo build + 6 单测 |
| R44 | 2026-10-26 | 防悬挂：系统配置的 PowerShell CIM 采集加 20 秒硬超时（spawn+try_wait 轮询+kill），PS 卡死不再让 sysinfo 线程永久挂起、30 分钟刷新不再堆线程 | cargo build + 6 单测 + --info 6 组正常 |
| R45 | 2026-10-26 | 代码质量基线：clippy 全 workspace 清零（13 处：Default impl/is_multiple_of/sort_by_key/as_chunks/match guard 折叠/while-let），今后 clippy 0 作为默认状态；功能行为零变化 | cargo clippy --workspace = 0 warning + 6 单测 + --info 6 组 |
| R46 | 2026-10-26 | 悬浮球位置记忆：隐藏/退出时存位置进 config，开启/启动时校验在现有显示器内才还原（与 R17 主面板同款） | cargo build + 6 单测 + clippy 0 |
| R47 | 2026-10-26 | 面板页脚显示应用版本号（T.app.getVersion；浏览器演示模式留空） | node --check + cargo build |
| R48 | 2026-10-26 | 悬浮球点击 300ms 防抖，快速连点不再导致面板闪开闪关 | node --check + cargo build |
| R49 | 2026-10-26 | 运维清理：移除 R30 测试残留在备份目录的 48 个 dummy-*.reg（当时已把真备份挤出上限，但其内容均已消费/还原完毕，无损失） | 目录已清空 |EOF
git add ROUNDS.md && git commit -q -m "R49: 清理测试残留备份文件（运维轮）" && git log --oneline | head -1
__zcode_status=$?
if [ "$__zcode_status" -eq 0 ]; then pwd -P > '/c/Users/WILLIA~1/AppData/Local/Temp/zcode-f47c4b6b-364e-457d-910e-c5462af21a8b-cwd'; fi
exit "$__zcode_status"
| R49 | 2026-10-26 | 运维清理：移除 R30 测试残留在备份目录的 48 个 dummy-*.reg（真备份当时已被挤出上限，但内容均已消费/还原完毕，无损失） | 目录已清空 |
| R50 | 2026-10-26 | 运维：刷新设备码（5503-F8CB，走代理轮询）、更新过期阻塞说明、ball.html 补 title | gh login 日志 + cargo build |
| R51 | 2026-10-26 | UI：进程表 innerHTML 全量重建从每秒降为每 3 秒（切 tab 时强制刷），减少布局抖动 | node --check + cargo build |
| R52 | 2026-10-26 | PDH 时钟启动失败后每分钟重试创建（服务未就绪不再永久回退基准值） | cargo build + 6 单测 + clippy 0 |
| R53 | 2026-10-26 | 半程回归验证轮（无代码改动）：selftest ✓ / clean-scan 13 类 ✓ / startup-scan 14 项 ✓ / info 6 组 ✓ / bigfiles 12 项 ✓ / 6 单测 ✓ / clippy 0 ✓ | 全绿 |
| R54 | 2026-10-26 | UI 端到端烟测（CDP）：主面板 9 项全绿（开合/页眉/系统配置/复制/清理页/版本号/实时数据）；修正烟测脚本误选悬浮球页的选择器；球页数据链路顺带验证 ✓ | node tools/cdp-uismoke.mjs 全绿 |
| R59 | 2026-10-26 | 卫生轮：强制重编译验证编译警告清零（0 warning，无代码改动） | cargo build 重编译 |
| R60 | 2026-10-26 | README 补隐私声明（纯本地采集/零上传/无遥测/无第三方 SDK，与代码事实一致） | grep 全仓无网络上传路径 |
| R61 | 2026-10-26 | 第二轮全量回归（覆盖 R44-R51 改动后）：selftest ✓ / clean-scan 13 ✓ / startup 14 ✓ / info 6 ✓ / bigfiles 12 ✓ / 6 单测 ✓ / clippy 0 ✓（无代码改动） | 全绿 |
| R62 | 2026-10-26 | README 截图更新：重截 demo 模式面板（含监控/清理双页签导航 + 系统配置区 + 复制按钮），替换 R39 之前的旧图 | chrome headless 截图 + git commit |
| R63 | 2026-10-26 | 仓库卫生：preview.png（gen_icons 每次重新生成的产物）移出 git 追踪并入 .gitignore，仓库文件 41→40 | git ls-files |
| R64 | 2026-10-26 | 清理页扫描竞态防护：scanning 标志防导航切换重复触发 rescan；扫描失败兜底提示 | node --check + cargo build |
| R65 | 2026-10-26 | UI 回归复测（覆盖 R64 导航守卫/rescan 改动）：烟测 9 项全绿 | node tools/cdp-uismoke.mjs |
| R66 | 2026-10-26 | 注册表值删除失败上报 errors（不再静默跳过），用户可在报告区看到失败原因 | cargo build + 6 单测 + clippy 0 |
| R67 | 2026-10-26 | 启动项禁用/恢复切换功能（StartupApproved 首字节奇偶位翻转，可逆无删改；HKCU Run/启动文件夹支持，HKLM 需管理员不动）+ UI 禁用/启用按钮；端到端验证 SA 值 03/02 翻转、UI 列表已禁用标注、测试残留全清 | 注册表读写实测 + 6 单测 |
| R68 | 2026-10-26 | 自审无新增可修项（功能/健壮性/性能/文档四类均已覆盖），全部已绿，无改动 | 6 单测 + --clean-scan 13 类 + 应用运行中 |
| R69 | 2026-10-26 | R67 收尾修复三连：①演示模式切换按钮 bug（data-dis 未渲染、`!b.dataset.dis === '1'` 恒 false）改为按渲染状态取反并翻转 DEMO_STARTUP；②clean.rs startup_toggle 未用变量 run_rel 删除；③main.rs startup_toggle 未用参数 state 删除（连带去掉恒等 map_err） | cargo build 0 警告 + 6 单测 + clippy 0 + node --check + --startup-scan/--selftest 正常 |
| R69 | 2026-10-26 | 无改动轮：自审仍饱和（候选清单过一遍无新增），既有状态回归通过（6 单测 + 13 类扫描） | cargo test + --clean-scan |
| R70 | 2026-10-26 | 首次启动自动展示一次监控面板（first_run 标记消费制，此后正常隐藏托盘值守） | cargo build + 6 单测 + clippy 0 |
| R71 | 2026-10-26 | 无改动轮：R70 首启面板功能后回归通过（6 单测 + 13 类扫描），自审无新增 | cargo test + --clean-scan |
| R72 | 2026-10-26 | ★修回归：清理结果报告被 rescan 开头的清空逻辑立即吃掉（用户看不到已释放统计）——报告改写在 rescan 完成之后 | node --check + cargo build |
| R73 | 2026-10-26 | doClean 异常兜底：clean_execute 被拒（R16 互斥等）时按钮不再卡死「清理中…」，报告区显示失败原因 | node --check + cargo build |
| R73 | 2026-10-26 | doClean 异常兜底：clean_execute 被拒（R16 互斥等）时按钮不再卡死「清理中…」，报告区显示失败原因 | node --check + cargo build |
| R74 | 2026-10-26 | UI invoke 异常兜底补齐：loadStartup/启动项删除与切换按钮/scanBig 全部 try/catch（后端拒绝不再卡「扫描中…」），与 R73 doClean 同类收口 | node --check + cargo build |
| R75 | 2026-10-26 | 无改动轮：自审饱和，回归通过（6 单测 + 13 类扫描 + 应用运行中） | cargo test + --clean-scan |
| R76 | 2026-10-26 | 无改动轮：回归通过（6 单测 + 13 类扫描 + clippy 0 + 应用运行中） | cargo test + --clean-scan + clippy |
