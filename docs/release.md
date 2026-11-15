# 发布与启动器规范（`matsuri-ai` / `matsuri-launcher`）

> Windows 便携启动器 `matsuri-ai.exe`（`crates/matsuri-launcher/`，零业务依赖，子进程拉起 `bin/matsuri.exe`）的目录布局、四频道更新边界与签名发版流程。

## 1. 发布包布局与定位

便携分发，全量置于安装根目录（仅 Windows，无 GUI，不写配置文件与 `launcher.log`）：

```text
matsuri-ai/
  matsuri-ai.exe          # 启动器入口（crates/matsuri-launcher）
  LICENSE                 # CC BY-NC-SA 4.0
  data/                   # 静态资产、manifest.json 与默认配置
  bin/
    matsuri.exe           # 核心 CLI（crates/matsuri-cli）
  .portable/              # 小黑板目录（Plugins/*.zip, *.br, version.json）
```

- 启动器固定以安装目录为子进程 cwd（`DATA_ROOT = "data"`），通过 `--root` 传入游戏/插件根路径。
- 小黑板（`UmamusumeResponseAnalyzer.exe`）须以 `CREATE_NEW_CONSOLE` 独立窗口启动（Terminal.Gui 检测到重定向会直接退出，且需与在线模式并行）。

## 2. 菜单与非 TTY 接口

- **菜单定义**：见 [`crates/matsuri-launcher/src/main.rs`](../crates/matsuri-launcher/src/main.rs)（`ITEMS` / `UPDATE_ITEMS`）。
  - 主菜单编号：`1` 在线模式（默认，空行回车）/ `2` 启动小黑板 / `3` 检查小黑板连接（只读检测白名单帧 mtime，120 s 四档）/ `4` 更新与插件 / `0` 退出。
  - 更新子菜单编号：`1` 检查更新 / `2` AI 数据 / `3` 小黑板数据 / `4` 小黑板插件 / `5` 程序本体 / `0` 返回主菜单。
- **交互与退出**：`q` / EOF 返回或退出；功能分支结束及路径初始化失败后执行 `pause` 防闪退（主菜单直接退出不 `pause`）。
- **跳过启动自检**：stdin 非 TTY 或设置 `MATSURI_NO_UPDATE_CHECK` 时，跳过进入菜单前的并发联网检查。

## 3. 四频道更新与守卫边界

进入菜单前对四频道并发只读自检（连接 5 s、单请求 8 s；不自动下载、不写状态文件；`.portable/` 缺失或 404 时记「跳过」）：

| 频道 | 目标目录 | 守卫白名单 | 边界 / 禁止触碰 |
|---|---|---|---|
| `program` | 安装根 + `bin/` | 整包 zip（`matsuri-ai.exe` + `bin/matsuri.exe` + 6 个程序资产），清单严格相等 | 不含 URA exe、`.portable/**`、三张基础表 |
| `ai_data` | `<安装根>/data/` | 仅 `umaDB.json`、`cardDB.json`、`text_data_dict.json`（schema 读 `data/manifest.json`） | 不碰 `events.json`、`constants.json`、`*_config.toml`、`scenario_matsuri.json` |
| `ura_data` | `<安装根>/.portable/` | 仅 `.br` 根文件（拒绝子目录） | 不碰 `PluginData/`、`config.yaml` |
| `ura_plugins` | `<安装根>/.portable/` | 仅 `Plugins/<单段>.zip` 与根 `version.json` | 不碰 `PluginData/`；协议不表达删除，插件下线须手工清理 |

### 程序本体自更新与校验

1. 逐文件比对 `files[]`，全命中即「已是最新」。
2. 下载后校验 zip 尺寸与 sha256，解压拒绝清单外条目与越界路径，并对两个 `.exe` 校验 **Authenticode 指纹白名单**（自签 `CN=UmaAI`，不要求根证书链受信任）。
3. 替换顺序：程序资产 → `bin/matsuri.exe` → 启动器自身（先 `rename` 为 `.old`，拉起新进程 `MATSURI_UPDATED=1` 后退出；下次启动清理 `.old`）；任一步失败逆序回滚。

## 4. 打包、签名与发版

- **打包脚本**：[`scripts/package_launcher.py`](../scripts/package_launcher.py)
  - 组装 `matsuri-ai.exe`、`bin/matsuri.exe`、`data/`、`LICENSE` 与 `dist/extra/`（保留空目录）。
  - `--sign`：调用 `signtool`，证书与口令仅取环境变量 `UMA_SIGN_PFX` / `UMA_SIGN_PFX_PASSWORD`。
  - `--program`：生成 `dist/program/matsuri-app-<版本>-<sha8>.zip`（无密码）与 `update.json`（版本号取根 `Cargo.toml` `[workspace.package] version`）。
  - `--zip`：用 7-Zip 生成 AES-256 加密完整分发包 `Matsuri_AI-<版本>.zip`。
- **发版脚本**：`dist/release.ps1`（发版机本地脚本，`/dist/` 已在 `.gitignore` 忽略且当前工作区不含）
  - 流程：构建 → 打包签名 → 上传程序频道 → 上传数据频道 → 校验。
  - 顺序：**先传 zip，再传 `update.json`**；随后回读程序包比对字节数与 sha256（防 CDN 旧缓存）。
  - 升级 `ai_data` schema 时，须先就绪远端频道，用户侧先更程序再更数据。
