# tailsync

Mac 之间通过 Tailscale 内网做单向文件同步的桌面工具。两台机器各装一份，分别可以往对面推或者从对面拉，方向用界面上分开的两个按钮显式控制。底层是 rsync 走 Tailscale SSH。

## 前置条件

- 两台机器都安装并登录了同一个 Tailscale tailnet
- 两台机器都执行了 `tailscale set --ssh`，开启了 Tailscale SSH
- 两台机器都安装了 `rsync`（macOS 自带）

## 安装

两台 Mac 都执行下面任一种方式：

**方式一：DMG 安装**

下载 release 中的 `tailsync_*.dmg`，双击打开，把 tailsync.app 拖到「应用程序」。首次打开会被 Gatekeeper 拦截，到系统设置 → 隐私与安全性 里允许它。

**方式二：从源码构建**

````
git clone git@github.com:lookfree/tailsync.git
cd tailsync
npm install
npm run tauri build
open src-tauri/target/release/bundle/dmg/
````

## 开发

```bash
npm install
npm run tauri dev      # 开发
npm run tauri build    # 打包 release
cd src-tauri && cargo test --lib       # Rust 单测
npm test                                # 前端单测
```

## 配置文件

`~/Library/Application Support/tailsync/pairs.json`

可以在两台机器之间手动复制（注意调整 local_path / remote_path）。

## 设计文档

`docs/specs/2026-05-15-tailscale-sync-tool-design.md`

## 仓库

https://github.com/lookfree/tailsync
