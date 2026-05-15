---
status: draft
date: 2026-05-15
author: wuhoujin
---

# tailsync 设计文档

## 是什么

两台 Mac 之间通过 Tailscale 内网做文件同步的桌面工具。每台 Mac 装一份，各自能往对面"推"（A→B）或者从对面"拉"（B→A），方向手动切换，单次只走一个方向，不存在双向自动同步的复杂度。同步底层是 rsync 走 Tailscale SSH，GUI 用 Tauri (Rust + Vue) 实现。

- 工具名：tailsync
- 应用 ID：`com.wuhoujin.tailsync`
- 二进制名：`tailsync`
- 配置目录：`~/Library/Application Support/tailsync/`

## 目标和非目标

目标是这些：

- 在已加入同一个 tailnet 的两台 Mac 之间，按用户定义的"目录对"做单向文件同步
- 任意一台机器都能作为发起方
- 同步方向通过界面上分开的"推过去 / 拉回来"两个按钮切换
- 支持断点续传、压缩、超时检测、可选限速
- 同步前必须 dry-run 预览，用户确认才执行
- 支持 .gitignore 风格的排除规则

下面这些明确不做：

- 不做双向自动同步（这是 Syncthing 的活儿，复杂度差几个量级）
- 不做后台文件监听 / 实时同步
- 不存同步历史 / 审计日志，只保留最近一次状态
- 不做配置在两台机器之间的自动同步（手动复制 JSON 即可）
- 不支持非 Tailscale 网络
- 不支持多于两台机器的拓扑（数据模型允许，但 UI 只针对两机场景设计）

## 整体架构

两台 Mac 上各装一份独立的 tailsync app，**两个 app 之间没有任何通信**。每个 app 只对它所在机器的本地配置和 GUI 负责。

发起一次同步的全流程是这样的：用户在 A 上点击某个目录对的"推过去"按钮，A 的 tailsync 起一个 `rsync --dry-run ...` 子进程，目标地址是 `用户名@对端 Tailscale 主机名`，rsync 通过 Tailscale SSH 连到 B 的 sshd，算出本次同步会复制 / 删除 / 修改哪些文件。tailsync 解析 rsync 的输出，弹确认窗给用户。用户确认后再起一个真实的 rsync 子进程，参数同上但去掉 `--dry-run`，实时解析进度推到前端，完成后更新最近一次状态。

"拉回来"同理，只是 rsync 的源和目标对调（源是 `用户名@对端:远端路径`，目标是本地路径）。

B 这边的 tailsync 在 A 推送过程中完全不参与，甚至可以没启动。文件能不能写到 B 上，取决于 B 的 sshd 是否在跑，以及 SSH 登录后的那个用户对目标路径是否有写权限。Tailscale SSH 在 tailnet 内默认免密互信，所以这块基本"开箱即用"。

为什么不让两个 app 互相通信？因为没必要。所有同步动作都是 rsync over SSH 一次性完成，加一层 app-to-app 协议只会增加故障点、增加协议演进负担、增加首次配置成本（互相发现、互相握手），不带来任何用户能感知的价值。如果未来想加"对端在线指示器""对端剩余磁盘空间"这种花活，再加一层轻量 HTTP 心跳即可，默认不做。

## 数据模型

核心实体只有一个：**目录对（DirectoryPair）**。

字段如下：

- `id`：UUID
- `name`：用户起的备注名，比如"读书笔记"
- `local_path`：本机绝对路径
- `remote_host`：对端 Tailscale 主机名（不是 IP，让 MagicDNS 做解析）
- `remote_user`：对端登录用户名，默认填当前用户名
- `remote_path`：对端绝对路径
- `excludes`：字符串数组，每项是一个 glob 模式（.gitignore 风格）
- `bandwidth_limit_kbps`：可选整数，0 或 null 表示不限速
- `mirror_mode`：布尔，是否传 `--delete`（默认 false，开启后才会让对端真正镜像本地）
- `last_sync`：可选对象，`{ direction: "push" | "pull", timestamp, status: "success" | "failed" | "interrupted", message }`

配置文件 `~/Library/Application Support/tailsync/pairs.json` 是一个 `DirectoryPair[]`。每次增删改保存时整文件重写，配上原子重命名（先写 `.tmp` 再 `rename`），不用考虑并发，因为只有 GUI 一个写入方。

## 主要界面

应用启动后只有一个主窗口，竖向布局。

顶部状态栏显示"本机：{Tailscale 主机名}"、"Tailscale 状态：已连接 / 未连接"、一个齿轮按钮入口走全局偏好（默认排除规则、默认限速等）。

下面是目录对列表，每行展示：备注名、`本地路径 ↔ 对端主机名:远端路径`、上次同步信息（带方向，比如"推过去 · 2 小时前 已完成"或"拉回来 · 3 天前 已中断"），最右边两个按钮："推过去"和"拉回来"。这两个按钮**必须物理上分开摆放**，不允许做成单按钮+方向开关。原因前面解释过：方向搞错的代价太大，肌肉记忆比节省一个按钮重要。

列表底部是"+ 新建目录对"按钮，点击进入新建表单：备注名是文本框；本机路径调系统原生文件选择器；对端机器是下拉框，内容来自启动时和定期刷新时调 `tailscale status --json` 解析出的 tailnet 节点列表；对端登录用户文本框默认填当前用户名；对端路径文本框失焦后悄悄走一次 `ssh {host} ls {path}` 验证存在性，存在显示绿色对勾，不存在标红（仅视觉提示，不阻止保存，因为用户可能想先存配置、首次同步时再让工具帮忙创建目录）；排除规则是 textarea，每行一个 glob 模式，UI 上提示"如 .DS_Store / node_modules/ / *.tmp"；限速是可选数字输入；镜像模式是 checkbox，配文案"开启后会从对端删除本机已删除的文件（危险操作）"。

点击某个目录对的"推过去"或"拉回来"按钮后，弹出一个模态对话框。标题是"准备推送：读书笔记 → mac-mini:/Users/wuhoujin/sync/notes"。中部展示 dry-run 结果摘要：复制 N 个文件（共 X MB）、删除 M 个、修改 K 个，可展开查看完整文件列表。底部按钮是"取消"和"确认执行"。

确认后该对话框变成实时进度视图：当前文件名、单文件进度、总进度条、瞬时速率、已传 / 总量。带"取消"按钮，点击会发 SIGTERM 给 rsync 子进程，已传的部分文件保留在对端 `.rsync-partial` 目录里，下次同步自动续传。

完成后对话框关闭，主界面对应行的"上次同步"刷新成新结果。失败的话对话框不自动关闭，错误信息原文展示，下方有"重试"和"复制错误"两个按钮。

## 同步执行机制

rsync 参数基线：

```
rsync -az \
  --partial \
  --partial-dir=.rsync-partial \
  --info=progress2 \
  --stats \
  --timeout=300 \
  --exclude-from=<临时文件> \
  [--delete] \
  [--bwlimit=N] \
  [--dry-run] \
  <源> <目标>
```

每一项的意义：

- `-a`：归档模式，保留权限、时间戳、符号链接、递归
- `-z`：传输时压缩，对慢网络收益明显
- `--partial --partial-dir=.rsync-partial`：中断后保留半截文件到 `.rsync-partial/`，下次自动续传，前传字节不重复
- `--info=progress2 --stats`：吐机器友好的进度信息（已传字节、总字节、瞬时速率、ETA），结尾吐总体统计
- `--timeout=300`：5 分钟没动静就当连接死亡，主动断开报错而不是吊死
- `--exclude-from`：从临时文件读排除规则，文件内容是 `excludes` 数组按行连接 + 全局默认排除项
- `--delete`：仅 `mirror_mode=true` 时启用
- `--bwlimit`：仅 `bandwidth_limit_kbps > 0` 时启用

全局默认排除项（用户不可关闭，避免脏文件）：

```
.DS_Store
.Spotlight-V100
.Trashes
.fseventsd
.TemporaryItems
._*
.rsync-partial/
```

dry-run 与真实执行**用完全相同的参数集**（只差 `--dry-run`），确保用户看到的预览和实际行为一致，不会"预览说没事、真跑出问题"。

进度解析方面，rsync 的 `--info=progress2` 输出格式约定良好（每行覆盖式刷新，字段为字节数、百分比、速率、ETA），Rust 端用一个状态机解析 stdout（按 `\r` 切分），把进度事件通过 Tauri 的 event 机制推给前端。前端不需要做任何字段解析，直接订阅 `sync-progress` 事件。

子进程管理：每个 sync 任务对应一个 Rust 端 `tokio::process::Command` spawn 出来的 rsync 进程，进程句柄保存到一个 `HashMap<TaskId, Child>`。"取消"操作向句柄发 SIGTERM；进程 exit 时回收，把 exit code 和最后的 stderr 摘要写进 `last_sync`。

## 错误处理

错误分两类：**用户可处理的**（提供按钮直接修复）和**用户不可处理的**（只展示信息让用户判断）。

可处理的有几种典型场景。远端路径不存在时，dry-run 阶段就能识别（rsync 报错），错误窗里给"自动创建远端目录"按钮，背后跑 `ssh {host} mkdir -p {remote_path}`。macOS 完全磁盘访问权限不足时，给"打开系统设置 → 隐私与安全性 → 完全磁盘访问"按钮，背后调 `open "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"`。Tailscale 未连接 / SSH 未启用时，状态栏标红提示，给"打开 Tailscale 偏好"或"在终端执行 `tailscale set --ssh`"的指引。

不可处理的，统一弹错误窗，结构是"一句白话翻译 + 折叠区域展示 stderr 原文 + 复制按钮"。比如 SSH 认证失败提示"无法登录 mac-mini，请确认 Tailscale SSH 已启用且双方 tailnet 互信"；对端磁盘满提示"对端剩余空间不足，需要至少 X 才能完成"；网络中断提示"传输中断，已传 X / 总 Y。点击重试将从中断处续传"。

## 首次启动环境检测

应用启动时跑一个三步检查，跑通才进主界面，否则停在引导页（带"重新检测"按钮）。

先 `which tailscale` 看 Tailscale CLI 是否在 PATH 中；不在就提示"未检测到 Tailscale CLI，请先安装"，给一个跳转 tailscale.com/download 的按钮。

接着 `tailscale status --json` 看是否登录 tailnet；未登录提示用户去菜单栏 Tailscale 登录。

最后从同一个 `tailscale status --json` 的输出里解析自身的 capabilities，确认 SSH 已启用；未启用提示用户在终端执行 `tailscale set --ssh`（不能由应用自己执行，这命令需要 sudo 提权）。

## 测试策略

不搞重型 e2e，按层次划分。

Rust 后端的纯逻辑（rsync 参数构造、进度行解析、排除文件生成、`pairs.json` 读写）用 `cargo test` 写单元测试。每个 pure function 配一组典型输入输出。

Vue 前端的关键组件（目录对表单校验、dry-run 摘要展示）用 Vitest 写组件测试，模拟 Tauri 命令调用。

GUI 整体流程（新建目录对 → dry-run → 确认 → 同步 → 完成）手工跑一遍验收。不值得为个人工具搞 Playwright。

跨网络真实场景（断网、超时、续传、限速）在两台真实 tailnet 节点上手工验证一次，跑通就行。

## 后续可能扩展（默认不做）

为了避免 YAGNI，下面这些一律先不做，留个注脚备查：

- 配置在两台机器之间自动同步（先靠手动复制 JSON）
- 同步历史 / 审计日志
- 后台监听 / 定时自动同步
- 对端 app 在线状态指示器
- 三台以上设备的拓扑管理
- Windows / Linux 支持
- 配置加密存储（`pairs.json` 里都是路径名，不敏感，明文存）
- 通知中心提醒同步完成
