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
| master.mdb `single_mode_15_*` 导入 | 已解码，未导入 | 静态数据 | `docs/plan/intake.md` §3 / §4 | S2 数据层按表 schema 导入 / 转换 | 会位移 | TBD（导入） |
| `scenario_id` | `15` | data `single_mode_scenario` | `data/scenario_matsuri.json` | 已确认 | 无 | DONE |
| gauge 归属规则 | 未实现 | memo / official | `game/*` | 友情 +2 范围 / 休息与非目标比赛加哪条（待真机） | 会位移 | TBD |
| 合宿延长规则 | 未实现 | user（2026-10-26） | `game/stage.rs` | 每年保证 4 个非比赛合宿回合；比赛 / 强制休息顺延；具体边界算法待落 | 会位移 | TBD |
| 合宿期支援卡事件抑制 | 未实现 | user（2026-10-26） | `game/event.rs` | 正常合宿不触发支援卡事件；延长段是否触发连续事件待测 | 会位移 | TBD |
| 合宿 Lv +1 结算点 | 未实现 | user（2026-10-26） | `game/stage.rs` / `game/event.rs` | 8 月后半结束事件 vs 全部顺延结束（待测） | 会位移 | TBD |
| 友人补位分身 | 单次补位 | memo / official | 人头布局 | 是否多位置分身（待真机） | 会位移 | TBD |
| 同名双人头 | 可并存 | memo | 人头布局 | 编成内 + 编成外是否去重（待真机） | 会位移 | TBD |
| `check_point_effect 204` | 推断レースボーナス | data | 剧本资产 | 本番奖励语义（待真机） | 会位移 | TBD |
| `act_effect 201` 的 value 30 | 保留原值 | data | 剧本资产 | 隐藏参数语义（待真机） | 可能位移 | TBD |
| 事件触发回合 | 仅标题 | data / memo | `game/event.rs` | 故事脚本 / 正式帧复核 | 会位移 | TBD |
| 追加稽古 gauge 增量 | 未知 | memo | 剧本资产 | 真机事件结算 | 会位移 | TBD |

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
