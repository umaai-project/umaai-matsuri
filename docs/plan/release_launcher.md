# 发布启动器（`matsuri-ai`）计划

> 状态：**基础版已实现**（2026-11-07）；同日补「启动小黑板」与头部信息改版。本文保留完整版形态与边界，
> 作为后续阶段参考；基础版与本文的差异见 §0，**以 §0 / §0.1 / §0.2 为实现现状**。
> 决策（2026-11-07）：启动器名 `matsuri-ai`；其他二进制放 `bin/`；发布只考虑 Windows；
> 不做 GUI；**核心 CLI 不改**。
>
> 数据更新已接入（2026-11-09）：菜单新增「更新数据（联网）」，见 §0.1。
> 程序更新与 Authenticode 签名已接入（2026-11-09）：菜单新增「更新程序（联网）」，见 §0.2。

## 0. 基础版实现记录（2026-11-07）

按「做减法」重排后的基础版，与本文完整版计划的差异：

| 项 | 本文完整版 | 基础版实现 |
|---|---|---|
| 菜单 | 6 项（在线 / 离线 / 日志 / 设置 / 版本 / 退出） | 4 项：在线（默认）/ 启动小黑板 / 检查小黑板连接 / 退出；2026-11-09 追加「更新数据（联网）」→ 5 项，见 §0.1 |
| 头部 | 未规定 | 标题 + 分隔线 + 四行只读信息（安装目录 / 游戏根 / 小黑板 / 核心程序）；标签按显示宽度对齐，状态点按文件在位染色，VT 不可用自动退纯文本；控制台窗口标题设为 `骏大祭 AI v<版本>` |
| 小黑板启动 | 无（由用户手工开 URA） | 取同目录 `UmamusumeResponseAnalyzer.exe`，`CREATE_NEW_CONSOLE` 另开窗口，不等待；仅打印三步引导，不代点「启动」、不改 URA 配置 |
| 离线 | 3 档 + 进阶、帧回放 | 整个砍掉，后续另议 |
| 配置 | `data/config.toml` + 设置菜单 | 不做；游戏根默认取启动时 cwd，无插件目录回退安装根；不写入任何文件 |
| 日志 | `logs/` + `launcher.log` | 不做 |
| 打包 | 后续阶段 | `scripts/package_launcher.py` 已落地：`matsuri-ai.exe` + `bin/matsuri.exe` + `data/`，附加资源递归取自 `dist/extra/`（保留空目录），缺失 URA 时给告警；`--zip` 产出带口令的 `Matsuri_AI-<版本>.zip`（7-Zip AES-256） |
| 通道体检 | 无 | 独立只读模式：查插件目录与白名单帧 mtime，120 s 四档结论；不启动子进程、不解析帧内容 |

小黑板为何必须另开窗口：URA 是 Terminal.Gui 常驻 TUI，`Program.cs` 检测到 stdin / stdout
被重定向即报错退出；而同窗口启动会占死启动器菜单，导致无法再进「在线模式」——两者必须同时在跑。
URA 自身的便携判定是 `exe 旁 .portable 目录`，与启动器传入的 cwd 无关，故不代设工作目录。

### 0.1 数据更新（2026-11-09）

按「做减法」接入的最小数据更新：

| 项 | 实现 |
|---|---|
| 菜单 | 第 4 项「更新数据（联网）」；非 TTY 编号 `4`，退出仍 `0` |
| 协议 | 引擎 `umaai-update` 的 `update.json`（逐文件 sha256、全量校验后替换、失败回滚） |
| 频道 | `ai_data` → `<安装根>/data/`；`ura_data` → `<安装根>/.portable/` |
| 守卫 | `ai_data` 只允许 `umaDB.json` / `cardDB.json` / `text_data_dict.json`；`ura_data` 只接受 `.br` 根文件 |
| 不更新 | `events.json` / `constants.json` / `ai_config.toml` / `matsuri_config.toml` / `scenario_matsuri.json` |
| 联网 | 只在选中该项时检查；启动不联网、不自动更新 |
| 发布侧 | UmaSimData 的 `make_version.py` 生成各频道 `update.json`，随 `coscmd upload -rs . /uma/` 上传；旧 `version.toml` 保留 |
| 生效 | 数据在核心 CLI / 小黑板启动时载入，更新后需重新启动对应进程 |

不自更新 `matsuri-ai.exe` / `bin/matsuri.exe`；不做断点续传、镜像与静默更新。

### 0.2 程序更新与签名（2026-11-09）

| 项 | 实现 |
|---|---|
| 菜单 | 第 5 项「更新程序（联网）」；非 TTY 编号 `5`，退出仍 `0` |
| 协议 | `<基址>/program/update.json`：`protocol` + `package{name,size,sha256}` + `files[]`（逐文件 size / sha256） |
| 载荷 | 整包 zip（约 3.3 MB）：`matsuri-ai.exe` + `bin/matsuri.exe` + 6 个程序资产 |
| 不含 | 小黑板本体（URA exe）、`.portable/**`（含 Plugins）、三张基础表（归 `ai_data` 频道） |
| 判定 | 无状态：先按 `files` 与本地逐文件比对，全部命中即「已是最新」，不下载整包 |
| 校验 | zip 尺寸 + sha256；解压后逐文件复核；拒绝清单外条目与越界路径；两份 exe 校验 Authenticode 指纹 |
| 指纹 | 自签 `CN=UmaAI`；客户端内置指纹白名单，不要求证书链可信（自签根不受信任属预期） |
| 落地 | 程序资产 → 核心 exe → 启动器；逐文件备份为 `.old`，任一失败按逆序回滚 |
| 自更新 | 启动器有变化时 `rename` 自身为 `.old` → 新 exe 就位 → 拉起新进程（`MATSURI_UPDATED=1`）→ 本进程退出；下次启动清理 `.old` |
| 清理 | 成功后删 `.umaai-update/program` 与空 `.umaai-update`；非启动器 `.old` 当场删除，启动器 `.old` 由新进程重试删除（最多 0.5 s）；失败保留临时目录供排查 |
| 发布 | `scripts/package_launcher.py --sign --program`：先签名两个 exe，再生成 `dist/program/matsuri-app-<版本>-<sha8>.zip` 与 `update.json`；包名带内容哈希，同名不同内容不会命中 CDN 旧缓存；版本号默认取 `Cargo.toml` 的 `[workspace.package] version`（与界面一致） |
| 签名 | `signtool sign /fd SHA256 /td SHA256 /tr <时间戳> /f <pfx> /p <口令>`；pfx 路径与口令只从环境变量读取，不入库 |
| 联动 | 数据频道 schema 门控改为读 `data/manifest.json`，程序升级提升 schema 后不再依赖启动器常量 |

不做：断点续传、镜像、静默更新、清单签名（Authenticode 指纹已覆盖执行前校验）。

### 0.3 一键发版脚本（`dist/release.ps1`，不入库）

把「构建 → 打包签名 → 上传程序频道 → 上传数据频道 → 校验」串成一条命令：

```powershell
powershell -ExecutionPolicy Bypass -File dist/release.ps1
```

| 项 | 说明 |
|---|---|
| 位置 | `dist/release.ps1`；`/dist/` 已被 gitignore，**不进版本库** |
| 口令 | 不落盘：优先 `UMA_SIGN_PFX_PASSWORD` / `UMA_ZIP_PASSWORD`，缺失时交互输入（仅驻留内存） |
| 证书 | `-Pfx` > `UMA_SIGN_PFX` > 内置路径（路径非敏感） |
| 版本 | 缺省取 `Cargo.toml` 的 workspace 版本；`-Version` 可覆盖 |
| 开关 | `-SkipBuild` / `-SkipSign` / `-SkipUpload` / `-SkipData` / `-Zip` / `-ProgramOut` |
| 完整包 | `-Zip` 产出 `Matsuri_AI-<版本>.zip`：7-Zip AES-256 口令保护（文件名不加密），口令取 `UMA_ZIP_PASSWORD` 或交互输入；版本取 Cargo workspace 版本；程序更新包 zip 保持不加密（客户端内置 zip 读取不支持 AES） |
| 上传顺序 | 先 zip 后 `update.json`，避免客户端拿到不存在的包 |
| 远端核对 | 上传后立即回读程序包，比对字节数与 sha256；不一致即失败（典型原因：CDN 缓存了同名旧包） |
| 校验 | 拉三个频道的 `update.json`，HTTP 200 + 打印版本与文件数 |
| 失败 | 任一步非零退出即抛错停止；不会产出半套清单 |

不变：名称 `matsuri-ai.exe`、仅 Windows、不做 GUI、**核心 CLI 不改**、
子进程 cwd 固定为安装根（`DATA_ROOT = "data"` 相对 cwd）、`--root` 指向游戏根、
错误与退出后 `pause` 防闪退。

实现落点：`crates/matsuri-launcher/`（交互选择用 `inquire`，不依赖本仓业务 crate）、`scripts/package_launcher.py`、
`dist/release.ps1`（本地一键发版，不入库）、根 `Cargo.toml` 第 6 个 member、`.gitignore` 的 `/dist/`。

## 1. 目标与边界

- Windows 用户双击 `matsuri-ai.exe`，默认进入**在线模式**，不需要记命令行。
- 离线模拟、手动核对、日志、游戏根目录设置从菜单进入。
- 核心 CLI 保持原样：`matsuri online ...` 仍是在线，`matsuri <trainer> ...`
  仍是离线；不改默认命令、不引入第二套语义。
- 启动器只做菜单、配置、子进程装配与收尾；决策逻辑仍在 `matsuri` / `matsuri-ai`。

## 2. 发布包布局

全部位于安装目录，便携形态：

```text
matsuri-ai/
  matsuri-ai.exe          # 启动器（发布唯一入口）
  LICENSE                 # 许可证（CC BY-NC-SA 4.0，随包分发）
  data/
    config.toml           # 启动器配置（本计划新增）
    ai_config.toml        # 引擎默认层
    matsuri_config.toml   # 剧本默认层
    constants.json
    ...                   # 其余数据表
  bin/
    matsuri.exe           # 核心 CLI
    ...                   # 其他随包二进制（首版只需 matsuri.exe）
  logs/
```

- 二进制名 `matsuri-ai.exe`；实现 crate 建议叫 `matsuri-launcher`，避免和
  `matsuri-ai` 库 crate 混淆。
- 启动器自身在安装根；其他二进制统一在 `bin/`。
- `matsuri-tools` 是否随包另定；随包的话同样放 `bin/`。
- 安装目录即工作目录；启动子进程前把 `current_dir` 切到安装根。

## 3. 菜单

```text
骏大祭 AI

  1. 在线模式（默认，回车）
  2. 离线模拟
  3. 打开日志
  4. 设置游戏根目录
  5. 显示版本
  0. 退出
```

### 3.1 在线模式

- 读取 `data/config.toml` 的 `game_root`；为空时提示输入，并校验目录含 `.portable`。
- 调用：

```text
bin\matsuri.exe online --root <game_root>
```

- 搜索预算仍由 `[search].search_n` 决定，不在启动器里改。
- 首版不暴露 `--json`；回放 / 调试放在“离线模拟”的进阶项或直接命令行。

### 3.2 离线模拟

```text
1. 默认训练员（matsuri）
2. 随机基线（random）
3. 手动核对（manual）
4. 进阶：--explain / --deck-profile
0. 返回
```

实际调用保持核心 CLI 原样：

```text
bin\matsuri.exe matsuri
bin\matsuri.exe random
bin\matsuri.exe manual
bin\matsuri.exe matsuri --explain
bin\matsuri.exe matsuri --deck-profile <name>
```

### 3.3 日志 / 设置 / 版本

- 打开日志：`logs/`；启动器自身写 `logs/launcher.log`。
- 设置：游戏根目录、`last_mode`；写回 `data/config.toml`。
- 版本：启动器版本 + `bin\matsuri.exe --version` + 数据版本（如清单中有）。

## 4. 配置

`data/config.toml` 是启动器配置，不是引擎的 `ai_config.toml` / 剧本的
`matsuri_config.toml`：

```toml
game_root = "C:/path/to/umamusume"
last_mode = "online"
```

- 放安装目录，不用 `%APPDATA%`，不需要管理员权限。
- 文件缺失或损坏时回退默认值，并备份原文件。
- 后续如果做数据更新，必须保留此文件，不随 `data/` 覆盖。

## 5. Windows 行为

- 控制台程序；子进程继承 stdin / stdout / stderr。
- 子进程退出或异常后 `pause`，避免双击窗口一闪而过。
- 设置 UTF-8 代码页；日志落 `logs/`。
- 开始菜单 / 桌面快捷方式指向 `matsuri-ai.exe`，起始位置设为安装目录。
- 不写注册表（除安装器创建快捷方式外），不需要管理员权限。
- 发布只验收 Windows；核心库在 Linux 的开发/测试不受影响。

## 6. 失败处理

| 场景 | 行为 |
|---|---|
| `game_root` 未配置 / 不存在 / 缺 `.portable` | 提示重新输入，不启动 online |
| `bin\matsuri.exe` 缺失 | 报错并提示检查安装包完整性 |
| 子进程非零退出 | 显示退出码，提示查看 `logs/`，`pause` |
| 配置损坏 | 备份后回退默认值，不阻塞启动 |
| 启动器自身异常 | 写 `logs/launcher.log`，提示手工运行 `bin\matsuri.exe` |

## 7. 实施阶段

1. **骨架**：新 crate / bin `matsuri-ai`，菜单、`data/config.toml`、根目录设置、
   在线/离线启动、日志、`pause`。
2. **离线进阶**：`--explain` / `--deck-profile` 子菜单；核心 CLI 参数不变。
3. **打包**：`bin/` 目录、图标、开始菜单快捷方式、发布说明。
4. **后续（不在本计划实现）**：GUI。

## 8. 验收

- 双击 `matsuri-ai.exe` → 菜单；回车进入 online，子进程使用
  `bin\matsuri.exe online --root <game_root>`。
- 离线菜单三档与核心 CLI 命令逐字一致；`--explain` / `--deck-profile` 可透传。
- 核心 CLI 行为、参数、测试不变。
- 安装目录移动后仍可用（全部路径相对安装根）。
- 「更新数据」：检查 → 确认 → 更新；断网 / 清单不合约定时不动文件；更新后重启核心生效。
- 「更新程序」：整包下载 → 摘要复核 → 指纹校验 → 备份替换 → 启动器自重启；未签名 / 指纹不符 / 篡改包均拒绝且不动文件。
- Windows 快捷方式指向启动器；错误后窗口停留；日志可打开。
- 非 Windows 不纳入发布验收。

## 9. 待拍板

- 首版是否包含离线“进阶”菜单，还是只保留在命令行。
- `data/config.toml` 是否还要存在线训练员 / 上次选择等字段。
- `matsuri-tools` 是否随发布包进入 `bin/`。
- 启动器图标 / 显示名。
- 版本号规则：程序包版本取 `Cargo.toml` 的 workspace 版本（2026-11-09 定）；每次发版需手动提升该版本。

## 10. 非目标

- 不改核心 CLI 的默认行为与参数。
- 首版未做数据更新；2026-11-09 已按 §0.1 接入。程序更新按 §0.2 接入。
- 不做 GUI。
- 不做非 Windows 发布形态。
- 不做游戏内插件或自动操作。
