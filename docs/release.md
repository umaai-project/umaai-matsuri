# 发布与启动器规范（`matsuri-ai` / `matsuri-launcher`）

> 本文档记录 Windows 便携启动器 `matsuri-ai.exe` 的目录布局、四频道更新边界与签名发版流程。
> 启动器 crate 位于 `crates/matsuri-launcher/`（零业务依赖，子进程拉起 `bin/matsuri.exe`）。

---

## 1. 发布包布局与定位

全部文件位于安装根目录，便携分发（仅 Windows，不做 GUI，不写配置文件与 `launcher.log`）：

```text
matsuri-ai/
  matsuri-ai.exe          # 启动器入口（crates/matsuri-launcher）
  LICENSE                 # CC BY-NC-SA 4.0
  data/                   # 静态资产、manifest.json 与默认配置
  bin/
    matsuri.exe           # 核心 CLI（crates/matsuri-cli）
  .portable/              # 用户侧小黑板目录（Plugins/*.zip, *.br, version.json）
```

- 启动器固定以安装目录为子进程工作目录（`DATA_ROOT = "data"` 相对 cwd），通过 `--root` 传入游戏/插件根路径。
- 小黑板（`UmamusumeResponseAnalyzer.exe`）必须用 `CREATE_NEW_CONSOLE` 另开窗口启动（Terminal.Gui 检测到 stdin/stdout 重定向会直接退出，且需与在线模式并行）。

---

## 2. 菜单与非 TTY 脚本接口

```text
主菜单（非 TTY 编号：1 2 3 4 0）
  1. 在线模式（默认，回车）
  2. 启动小黑板
  3. 检查小黑板连接（只读检测白名单帧 mtime，120 s 四档结论）
  4. 更新与插件（联网） · N 项可更新
     └ 1. 检查更新（只读刷新状态）
       2. 更新 AI 数据
       3. 更新小黑板数据（.br）
       4. 更新小黑板插件（Plugins/*.zip）
       5. 更新程序本体
       0. 返回主菜单
  0. 退出
```

- `q` / EOF 返回或退出；错误与退出后带 `pause` 防闪退。
- **跳过启动自检**：stdin 非 TTY（管道/脚本）或设置环境变量 `MATSURI_NO_UPDATE_CHECK`（任意值）时跳过进菜单前的并发联网检查。

---

## 3. 四频道更新与守卫边界

进菜单前在 `main()` 对四份清单做并发只读自检（连接 5 s、单请求 8 s；**不自动下载**、不写状态文件；`.portable/` 或 `Plugins/` 缺失及 404 时记「跳过」）。

| 频道 | 目标目录 | 守卫白名单 | 明确不碰 / 边界 |
|---|---|---|---|
| `program` | 安装根 + `bin/` | 整包 zip（`matsuri-ai.exe` + `bin/matsuri.exe` + 6 个程序资产），清单严格相等 | 不含 URA exe、`.portable/**`、三张基础表 |
| `ai_data` | `<安装根>/data/` | 仅 `umaDB.json`、`cardDB.json`、`text_data_dict.json`（schema 门控读 `data/manifest.json`） | 不碰 `events.json`、`constants.json`、`*_config.toml`、`scenario_matsuri.json` |
| `ura_data` | `<安装根>/.portable/` | 仅 `.br` 根文件（拒绝子目录） | 不碰 `PluginData/`、`config.yaml` |
| `ura_plugins` | `<安装根>/.portable/` | 仅 `Plugins/<单段>.zip` 与根 `version.json` | 不碰 `PluginData/`；**逐文件协议不表达删除**，插件下线需手工清理 |

### 程序本体自更新与安全校验

1. 客户端逐文件比对 `files[]`，全命中即「已是最新」。
2. 下载整包后校验 zip 尺寸与 sha256，解压校验拒绝清单外条目与越界路径，并对两个 `.exe` 校验 **Authenticode 指纹白名单**（自签 `CN=UmaAI`，不要求根证书链受信任）。
3. 替换顺序：程序资产 → `bin/matsuri.exe` → 启动器自身（`rename` 自身为 `.old` 后拉起新进程 `MATSURI_UPDATED=1` 并退出；下次启动清理 `.old`）；任一步失败按逆序从 `.old` 回滚。

---

## 4. 打包、签名与发版流程

- **本地打包脚本**：`scripts/package_launcher.py`
  - 组装 `matsuri-ai.exe`、`bin/matsuri.exe`、`data/` 与 `dist/extra/`（保留空目录）。
  - `--sign`：调用 `signtool`（pfx 路径与口令仅从环境变量 `UMA_SIGN_PFX` / `UMA_SIGN_PFX_PASSWORD` 读取，不入库）。
  - `--program`：生成带内容哈希的 `dist/program/matsuri-app-<版本>-<sha8>.zip` 与 `update.json`（版本号取根 `Cargo.toml` `[workspace.package] version`）。
  - `--zip`：使用 7-Zip 生成 AES-256 加密的完整分发包 `Matsuri_AI-<版本>.zip`（程序更新包 zip 本身保持无密码）。
- **一键发版脚本**：`dist/release.ps1`（`/dist/` 已被 `.gitignore` 忽略，不入库）
  - 串联「构建 → 打包签名 → 上传程序频道 → 上传数据频道 → 校验」。
  - 上传顺序：**先上传 zip，再上传 `update.json`**；上传后立即回读程序包比对字节数与 sha256（防 CDN 命中旧缓存）。
  - 若需更新 `ai_data` schema 版本，需**先确保远端频道就绪，且用户侧更新遵循先程序后数据**。
