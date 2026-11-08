# 卡组配置重构：`[decks.*]`（2026-11-07 拍板）

> 状态：已实施（2026-11-07）。本文是 `matsuri_config.toml` 的**单一事实源**说明；机制口径仍以
> [`../mechanism.md`](../mechanism.md) 为准，术语以 [`../glossary.md`](../glossary.md) 为准。

## 1. 背景

`matsuri_config.toml` 原来把两类东西放在同一层：

- **全局运行设置**：`[pre]`（局外切 / 心情盾）、`[search]`（搜索预算 / 并行度 / 作用面）；
- **一套按速主卡组调的剧本参数**：`[fusion]`、`[trainer]`，以及代码里的 `ROUTE_PRIOR` 教诲落位先验。

实测（dev plan #48，3000 局/档）耐 / 根 / 智主卡组比默认档低 3.9k–4.6k，失分集中在速度分项；
根因是 `status_weights`、`overflow`、`endgame`、教诲落位等都按速主卡组调。卡组参数需要按构成分档。

## 2. 方案：一个文件、两层配置

不新增配置文件；在 `data/matsuri_config.toml` 内把配置分成两层：

| 层 | 段 | 内容 |
|---|---|---|
| 全局运行设置 | `[pre]` / `[search]` | 局外切 / 心情盾、搜索预算 / 并行度 / 作用面 |
| 卡组参数 | `[decks.<name>]` | `comp` + 可选 `fusion` / `advice` / `trainer` |

- `[decks.default]` 是**完整默认档**（原 speed_build 参数），也是继承基座与未匹配兜底。
- 其他档只写与 default 的差异，深合并；21101 自动命中 default。
- `variant = true` 只用于扫参变体，必须按名字显式选，不参与自动匹配与兜底。
- 不提供玩家 deck 覆盖层；配置文件由维护者调整。

### 2.1 为什么不用「顶层 `[fusion]` / `[trainer]` 当 default + `[decks]` 覆盖」

顶层参数会同时扮演「全局默认」和「speed_build 卡组」两个角色，且继承规则不统一
（顶层整段、deck 深合并）。`[decks.default]` 显式后，所有档走同一条规则：**default 完整，其余深合并**；
读文件、扫参、校验只有一套语义。

## 3. 文件形态

```toml
[pre]
qie = "none"
positive_thinking = 0

[search]
search_n = 8192
threads = 0  # 搜索 rayon 全局池线程数；0 = 自动（显式 RAYON_NUM_THREADS 优先）
train = true
advice = true
event = true

[decks.default]
comp = [2, 1, 1, 0, 1]          # 速 / 耐 / 力 / 根 / 智 训练卡张数（合计 5）
note = "速主默认档；未匹配卡组走这里"

  [decks.default.fusion]
  year2 = [{ base = 101, target = 601 }, { base = 201, target = 701 }]
  year3 = [
      { base = 501, target = 801 },
      { base = 301, target = 901 },
      { base = 401, target = 1001 },
  ]

  [decks.default.advice]
  #        速   耐   力   根   智（基础 id：101 絆 / 201 得意率 / 301 ヒント / 401 助力 / 501 体力）
  t2  = [201, 101, 501, 301, 401]
  t11 = [501, 301, 101, 201, 401]
  t23 = [101, 501, 401, 301, 201]
  t35 = [101, 301, 501, 401, 201]
  t47 = [501, 101, 401, 301, 201]
  t59 = [501, 401, 101, 301, 201]

  [decks.default.trainer]
  pt_favor_rate = 1.7271
  ...                              # 原 [trainer] 全量
    [decks.default.trainer.weights]
    ...
    [decks.default.trainer.advice]
    ...
    [decks.default.trainer.dynamic]
    ...
    [decks.default.trainer.overflow]
    ...
    [decks.default.trainer.endgame]
    ...

[decks.stamina_build]
comp = [1, 2, 1, 0, 1]
note = "耐主；首版只占位，后续扫参再填"

[decks.guts_build]
comp = [2, 0, 1, 1, 1]

[decks.wisdom_build]
comp = [1, 1, 1, 0, 2]

# 扫参变体：同 comp，只写差异，必须按名字显式选
[decks.default__advB]
comp = [2, 1, 1, 0, 1]
variant = true

  [decks.default__advB.advice]
  t47 = [101, 501, 401, 301, 201]
```

### 3.1 合并与匹配规则

1. `default` 必须存在、非 variant，且 `comp` / `fusion` / `advice` / `trainer` 完整。
2. 其他档：`comp` 必填；`fusion` / `advice` / `trainer` 可选，深合并到 `default`：
   - `fusion`：`year2` / `year3` 分别整段替换；
   - `advice`：按窗口（`t2` … `t59`）替换，未写继承；
   - `trainer`：逐字段递归（含 `weights` / `advice` / `dynamic` / `overflow` / `endgame`）。
3. `comp` / `note` / `variant` 是条目元数据，不继承。
4. 非 variant 档的 `comp` 必须唯一；重复即载入报错。`default` 的 `comp` 也参与唯一性检查。
5. 自动匹配：按 `comp` 精确查非 variant 档；找不到 → `default`。
6. 显式选择：`--deck-profile <name>` 或 `MatsuriConfig::for_profile(name)`；`variant` 档只能这样选。
7. 教诲预设写**基础 id**（101–501）：転成后仍是同一槽，换融合方案不会让预设失效；每个窗口必须是
   `{101, 201, 301, 401, 501}` 的一个排列。

## 4. 类型与解析

```rust
pub struct MatsuriConfig {
    pub pre: PreConfig,
    pub search: SearchScopeConfig,
    pub decks: Arc<DeckTable>,     // 载入时解析完成：default + by_comp + by_name
    #[serde(skip)]
    pub deck: Arc<DeckParams>,     // 本次跑局的生效参数；文件里不出现
}

pub struct DeckParams {
    pub fusion: FusionConfig,
    pub advice: AdvicePreset,      // 六窗口基础 id
    pub trainer: TrainerConfig,
}

pub struct DeckTable {
    default: DeckParams,
    by_comp: BTreeMap<[usize; 5], DeckParams>,
    by_name: BTreeMap<String, DeckParams>,
}
```

- `DeckTable` 在反序列化时解析 `default`、深合并其余条目并校验；运行期 `for_deck(comp)` 只查表。
- `DeckParams::default()` 内置一份与 `[decks.default]` 相同的兜底（无文件 / 测试用），由
  `deck_config` 的测试逐字段锁死，避免两份默认漂移。
- `MatsuriConfig::for_deck(comp)` / `for_profile(name)` 返回已填充 `deck` 的配置；基座保持不可变，
  每次跑局 clone 再 resolve。`decks` / `deck` 用 `Arc` 共享，`MatsuriGame` 每次 rollout 克隆只拷指针，
  不深拷贝卡组表。
- `settle.rs` 读 `config.deck.fusion`；训练员 / `Model` 读 `config.deck.trainer`；
  `Model` 读 `config.deck.advice`。`ROUTE_PRIOR` 从 `value.rs` 删除，默认落位只存于
  `AdvicePreset::default()` 与 `[decks.default.advice]`。
- `config.rs` 只保留全局配置（`pre` / `search` / `MatsuriConfig` / overlay）；新增
  `deck_config.rs` 承载卡组参数、卡组表、`FusionConfig` / `TrainerConfig` / `AdvicePreset` 与校验。

## 5. 行为等价与首版内容

首版 `[decks.default]` 的内容 = 原 `[fusion]` + `[trainer]` + `ROUTE_PRIOR`；其余三套只写
`comp` / `note`，继承 default。因此：

- 默认档、21101、未匹配卡组的决策与现在**逐位一致**；
- 这是纯 schema 迁移，不重录基线；
- 后续扫参只动 `[decks.*]`，用 `variant = true` + 显式档名。

## 6. 迁移清单

| 项 | 处理 |
|---|---|
| `data/matsuri_config.toml` | `[fusion]` / `[trainer]` 移入 `[decks.default.*]`；新增 `[decks.*]` 占位档 |
| `config.rs` | 删除顶层 fusion / trainer 字段与 overlay；`MatsuriConfig` 增加 `decks` / `deck` |
| 字段读取 | `config.trainer` → `config.deck.trainer`；`config.fusion` → `config.deck.fusion` |
| 教诲先验 | `value.rs` 的 `ROUTE_PRIOR` 移入 `AdvicePreset::default()` / `[decks.default.advice]` |
| 入口 | `matsuri` / `bench` / `sweep` / `cardtest` / `audit_search` / `probe_fail` / `online` 在跑局前按 `comp` resolve；`cardtest` 每套卡组单独 resolve |
| 测试 | `parse_repo_default` / `load_repo_config` 改为「全局段 + default 档」比较；新增合并 / 变体 / 兜底 / 深合并用例 |
| lab 扫参脚本 | 顶层 `[fusion]` / `[trainer]` 模板改为 `[decks.default.*]`；仓库提供 `scripts/migrate_deck_config.py` 做一次性转换：`python3 scripts/migrate_deck_config.py old.toml --out new.toml`（批量：`--dir olddir --out-dir newdir`）；扫参选档用 `bench --deck-profile <名>` / `cardtest run` 的卡组行名（行名命中 `[decks.*]` 即显式选档） |
| 文档 | README 结构表、`mechanism.md` §6.1、`glossary.md`、`tests_overview.md` 同步 |

## 7. 验收

- `cargo fmt --all --check`；
- `cargo check --workspace --all-targets --offline`；
- `cargo test --workspace --release --offline`；
- 默认档冒烟（seed 61444）：迁移前后均为 **86053 / LG15、PT 11864**（逐位一致）；不需要重录基线。

## 8. 决策记录

| 日期 | 决策 | 理由 |
|---|---|---|
| 2026-11-07 | 一个文件、两层配置；`[decks.default]` 显式 | 避免顶层参数双语义；所有档一套继承规则 |
| 2026-11-07 | 教诲预设写基础 id | 融合方案变化、在线实际融合与配置不一致时仍能落位 |
| 2026-11-07 | 深合并（trainer 逐字段，fusion 分段，advice 分窗口） | 扫参变体只写差异；避免整块复制 |
| 2026-11-07 | `variant = true` 显式选择，不参与自动匹配 / 兜底 | 扫参档与生产档共存，不静默套错 |
| 2026-11-07 | 不做玩家 deck 覆盖层 | 卡组参数由维护者调整，避免半套覆盖 |
| 2026-11-07 | 保留 `DeckParams::default()` 内置兜底 | 无文件 / 测试直接可用；与 default 档由测试锁死 |

## 9. 实施结果（2026-11-07）

- `config.rs` 只保留 `[pre]` / `[search]` / `MatsuriConfig` / 覆盖层；新增 `deck_config.rs`
  （`DeckParams` / `DeckTable` / `AdvicePreset` / `FusionConfig` / `TrainerConfig` / 深合并 / 校验）。
- `settle.rs` 读 `config.deck.fusion`；`trainer.rs` / `search.rs` / `value.rs` 读
  `config.deck.trainer`；`Model` 读 `config.deck.advice`；`ROUTE_PRIOR` 已从 `value.rs` 删除。
- 入口：`matsuri`（离线）按 comp resolve 并支持 `--deck-profile`；`online` 只按 comp 自动匹配
  （玩家侧不再暴露卡组档覆盖）；`bench` 支持 `--deck-profile`；`cardtest run` 的卡组行名命中
  `[decks.*]` 时显式选档，否则按 comp；
  `sweep` / `audit_search` / `probe_fail` 在 `Env::load` 按 comp resolve。
- 验收：`cargo fmt --all --check` / `cargo check --workspace --all-targets --offline` /
  `cargo test --workspace --release --offline` 全绿（258 通过 + 2 手动 `#[ignore]`）；
  默认档 seed 61444 冒烟 **86053 / LG15、PT 11864**，与迁移前逐位一致。
- 迁移工具：`scripts/migrate_deck_config.py`（主仓旧 schema 单文件 / `--dir` 批量转换）。
  外部 `Matsuri_Workbench` 是独立旧 checkout，lab 分支的 `[trainer]` 带 `status_halve_*` /
  `friend_outing_*` 等主仓没有的字段；纯文本迁移会产出主仓无法解析的配置，因此未改归档文件。
  lab 侧需先 rebase 到新仓，再按本方案改模板 / 用 `--deck-profile` 选档。

## 10. 后续定卡与扫参（2026-11-07）

- 四套 build 的定卡、speed 换卡与重录基准、`fusion` / `advice` / 手写旋钮的扫描顺序见
  [`deck_param_scan.md`](deck_param_scan.md)。
- 玩家 CLI / online 仍不做 `[decks.*]` 覆盖；测卡 / 扫参工具新增测试专用
  `--matsuri-config <完整配置>`（直接 `MatsuriConfig::load(path, None)`），不接线到玩家侧。
