# 数值占位登记（STUB / TBD）

> 机制已确认、数值未到时的统一登记处。**占位不阻塞开发**，但必须可整体替换、可盘点。
> 规则：
> - 占位集中放在剧本资产 / 配置 / 常量表，代码里不散落魔法数；
> - 代码标记统一为 `STUB:`（临时占位）与 `TBD:`（待补逻辑/数值），并在此表登记；
> - 替换为确认值后：重录基线，并在 [`glossary.md`](../glossary.md) 的决定记录写明位移原因与影响面。

## 登记表

| 项 | 当前占位值 | 来源 | 存放位置 | 替换条件 | 基线影响 | 状态 |
|---|---|---|---|---|---|---|
| `sample_resource` | `0` | 模板占位 | `data/scenario_matsuri.json` / `MatsuriData` / `MatsuriView` | 5 条演目 gauge 账本定形（S2/S5） | 会位移 | OPEN |
| `training_basic_value` | 模板样例 2 档 × 5 训练 | 模板 | `data/scenario_matsuri.json` | 已从 `single_mode_training_effect`（scenario 15，56 行）提取，待 S2 导入 | 会位移 | RESOLVED（数值）/ TBD（导入） |
| `sample_order` | `[0, 1, 2, 3, 4]` | 模板 | `data/matsuri_config.toml` / `MatsuriConfig` | 手写训练员偏好表定形（S7） | 会位移 | OPEN |
| master.mdb `single_mode_15_*` 导入 | 已解码，未导入；机制包 v1 提供全量 `scenario_matsuri.json` | 静态数据 | `docs/plan/intake.md` §3 / §4 | S2 数据层按表 schema 导入 / 转换（注意机制包回合为 1 基） | 会位移 | TBD（导入） |
| `scenario_id` | `15` | data `single_mode_scenario` | `data/scenario_matsuri.json` | 已确认 | 无 | DONE |
| gauge 归属规则 | 未实现 | memo（2026-10-27） | `game/*` | 参与担当每人 +1 / 友情 +2 / 目标比赛 +4 / 休息外出 +1；按 Lv 与 gauge 最低者优先；彩圈范围待真机 | 会位移 | TBD（实现） |
| 合宿延长规则 | 未实现 | user（2026-10-26） | `game/stage.rs` | 每年保证 4 个非比赛合宿回合；比赛 / 强制休息顺延；具体边界算法待落 | 会位移 | TBD |
| 合宿期支援卡事件抑制 | 未实现 | user（2026-10-26） | `game/event.rs` | 正常合宿不触发支援卡事件；固定马娘事件仍发生；延长段待测 | 会位移 | TBD |
| 合宿 Lv +1 结算点 | 未实现 | user（2026-10-26） | `game/stage.rs` / `game/event.rs` | 8 月后半结束事件 vs 全部顺延结束（待测） | 会位移 | TBD |
| 友人补位分身 | 单次补位 | memo / official | 人头布局 | memo 未提分身；是否多位置（待真机） | 会位移 | TBD |
| 同名双人头 | 可并存 | memo | 人头布局 | 编成内 + 编成外是否去重（待真机） | 会位移 | TBD |
| `check_point_effect 204` | レースボーナス 30 / 100 | data / memo | 剧本资产 | 语义按 memo，数值待真机复核 | 会位移 | TBD（存疑） |
| `check_point_effect 202` | 友情加成 5 / 10 / 20 | data / memo | 剧本资产 | memo 20/20 行吻合 | 会位移 | RESOLVED（语义） |
| `check_point_effect 2` | 灵感发生率 20 / 50 / 100 / 150 | data / memo | 剧本资产 | memo 20/20 行吻合 | 会位移 | RESOLVED（语义） |
| 発表会「成功」档一次性奖励 | 只有大成功文案 | data / memo | 剧本资产 | 成功档全能力 / PT 实数（待官方 UI） | 会位移 | TBD |
| `act_effect 201` 的 value 30 | 保留原值 | data | 剧本资产 | 全等级恒定；隐藏参数语义（待真机） | 可能位移 | TBD |
| 助力支撑 `20 → 47` | 保留原值 47 | data | 剧本资产 | 47 的次数 / 系数语义；参考实现按 3 次 +5% | 会位移 | TBD |
| 属性基础上限口径 | 2200/1850/1800/1700/1800；突破后 2500 | official + memo | 剧本资产 / 常量 | 已定主口径；机制包 2150 一组不入代码 | 会位移 | RESOLVED |
| 演目稽古基础值口径 | 静态表按 `command_level = 演目 Lv`；memo 实测随时期 / 训练 Lv | data / memo | 剧本资产 / 训练结算 | 真机核对后定一套（S2 前置） | 会位移 | TBD |
| 训练公式下段 | memo TODO | memo | 训练结算 | 下段基础值与加成口径 | 会位移 | TBD |
| 目标赛事 SP 奖励 | memo +72？ | memo | 剧本资产 / 比赛结算 | 真机核对 | 会位移 | TBD |
| Senior 每演目 +1 担当 | memo 已答 | memo | 人头布局 | 真机复核 | 会位移 | TBD（复核） |
| 事件触发回合 | memo 时间轴已给；10 月前半待核 | data / memo | `game/event.rs` | 故事脚本 / 正式帧复核 | 会位移 | TBD（复核） |
| 追加稽古 gauge 增量 | +1（最低演目） | memo（2026-10-27） | 剧本资产 | 上 / 下选项完整效果已给出；真机复核 | 会位移 | RESOLVED（数值）/ TBD（实现） |
| 吾妻无凸 / R 连续事件 | 未知 | memo | 事件管线 | 官方 UI / 真机 | 会位移 | TBD |
| 通常训练 Lv 上升 | 4 回で上昇？ | memo | 训练结算 | 真机核对 | 会位移 | TBD |

## 代码标记约定

```rust
// STUB: 占位值，见 docs/plan/stubs.md
const SAMPLE_RATE: i32 = 100;
```

- `STUB`：值已存在但未确认，**可以跑**；
- `TBD`：逻辑或数据缺失，**需要后续补齐**（不要用它绕过机制未确认）。
- 标记只用于数值 / 数据占位；**机制未确认不得用 stub 绕过**（应按 S0 闸门先确认机制）。

## 收尾检查

- [ ] 所有 `STUB` / `TBD` 都已在本表登记；
- [ ] 每项都有替换条件与影响面；
- [ ] 替换后已重录基线并在 `glossary.md` 留痕；
- [ ] 代码中搜索 `STUB:` / `TBD:` 与登记表一致。
