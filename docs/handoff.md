# 会话交接

> 面向接续：只写当前状态、未决项与下一步，细节引用其他文档与 commit。

## 1. 当前目标

matsuri 复盘工具链已收口（代码 + skill），**下一步：细致调整复盘输出与人设**。

## 2. 已完成（近两日）

- 复盘工具：`stat` 收敛、`eval --swap`、逐决策面（`turn + seq`）、`whatif` 反事实（历史反推 / 多替代 / K=100 / 终局）、`report` 对比表，提交 `4232eb9`。
- 复盘 skill：`skills/matsuri_review/`（SKILL.md + reference/{metrics_glossary,pitfalls,commands} + 构建脚本）；`bin/review.exe` 预构建、运行期不依赖 cargo；exe 与 data 副本不入库。
- 口径修复：友人出行 `group=390`（原误判为休息）、出行回合统一命令回合、五维统一显示值、继承只给数字（内部标准 306）、事件选项比对停用、`ai.present` 只作内部判据。
- skill 发布态收口：文档只留发布态，构建/分发/源码信息移入 `reference/dev.md`（发布包删除）；`--data` 缺省改为按 exe 位置解析（skill 的 `data/` 优先），独立副本不传参也可运行。
- 深挖：`review show <局包> --turn N --seq S` 回放原始帧输出 JSON（数值 / 5 训练位收益 / 人头 / 事件；seq 必带）；`report.html` 末尾附「更优回合」表（regret ≥ 100）。
- 选择偏差 N 定档：日常 `eval --all --n 2048`，决赛点 `--n 8192`；叙事下限 100（100–200「小优」）。
- 修复教诲实际取帧：原取回合末帧（常是授权前），T59/T35 假显著；改取决策帧之后第一帧 slots 变化。6327 显著 4→2（T51/T4），6329 1→0。

## 3. 未决 / 阻塞

1. **事件选项比对**已停用（波动大）；恢复需先稳定采集与口径。
2. **追加稽古 11 次全不可判**——终局帧 `actions` 未采集该事件，属采集缺口。
3. **skill 的发现方式**：仓内 `skills/` 不被 DSH 自动发现，需扫描路径或软链（未定）。
4. **`bin/review.exe` 与 `data/` 不入库**：靠 `scripts/build_review_skill.ps1 -WithData` 生成；clone 后首次使用需跑一次构建。分发 = 整目录复制（含 `bin/` 与 `data/`），发布包删除 `reference/dev.md`。
5. **真机帧回放守门空档**：`matsuri-ai` 的 6 个仓外帧集成测试已删，仓内无替代 fixture。

## 4. 下一步（用户指定方向）

1. **复盘输出细化**：叙事结构、六问顺序、结论措辞（当前见 `skills/matsuri_review/SKILL.md` 与 `docs/matsuri_review.md`）。
2. **人设**：口吻/风格尚未定（旧 skill 有 `persona.md` + 背景图，新版刻意未纳入），需先拍板「平实 vs 拟人化」。
3. **report 剩余**：分数轨迹改 T(n) 主图（属性分标注「不作判据」）；`--narrative` 支持 markdown。

## 5. 关键入口

| 用途 | 位置 |
|---|---|
| 复盘工作流 / 口径 / 坑位 | `skills/matsuri_review/{SKILL.md,reference/}` |
| 复盘指南（人类可读） | `docs/matsuri_review.md` |
| 决策流水 | `docs/changelog.md` |
| 待办 | `docs/todo.md` |
| 工具源码 | `crates/matsuri-tools/src/review/` |

真机局包默认在 `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/`；复盘产物建议落在工作区 `game{id}/`。
