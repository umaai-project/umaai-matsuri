# 发布启动器（`matsuri-ai`）计划

> 状态：方案已拍板，**未实现**。本文只定形态与边界，不改代码。
> 决策（2026-11-07）：启动器名 `matsuri-ai`；全部放安装目录；配置放
> `data/config.toml`；其他二进制放 `bin/`；发布只考虑 Windows；更新工具暂不做；
> 不做 GUI；**核心 CLI 不改**。

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
4. **后续（不在本计划实现）**：数据更新工具；GUI。

## 8. 验收

- 双击 `matsuri-ai.exe` → 菜单；回车进入 online，子进程使用
  `bin\matsuri.exe online --root <game_root>`。
- 离线菜单三档与核心 CLI 命令逐字一致；`--explain` / `--deck-profile` 可透传。
- 核心 CLI 行为、参数、测试不变。
- 安装目录移动后仍可用（全部路径相对安装根）。
- Windows 快捷方式指向启动器；错误后窗口停留；日志可打开。
- 非 Windows 不纳入发布验收。

## 9. 待拍板

- 首版是否包含离线“进阶”菜单，还是只保留在命令行。
- `data/config.toml` 是否还要存在线训练员 / 上次选择等字段。
- `matsuri-tools` 是否随发布包进入 `bin/`。
- 启动器图标 / 显示名 / 版本号规则。

## 10. 非目标

- 不改核心 CLI 的默认行为与参数。
- 首版不做数据更新工具（后续另立计划）。
- 不做 GUI。
- 不做非 Windows 发布形态。
- 不做游戏内插件或自动操作。
