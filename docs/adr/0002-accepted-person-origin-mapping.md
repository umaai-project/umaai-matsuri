# ADR-0002: MatsuriPerson 委托与 PersonOrigin 单向映射

- **状态**：Accepted
- **日期**：2026-11-15
- **关联**：`matsuri-core/src/game/{person.rs, roster.rs}`、[ARCHITECTURE.md](../../ARCHITECTURE.md)

## Context

剧本内包含支援卡、友人、编成外神驹、理事长与记者等多种人物来源，其中社交人头与编成外神驹在规则判定中具有不同语义；若直接修改或扩展引擎侧 `PersonType` 枚举，会造成跨仓契约破坏。

## Decision

`MatsuriPerson` 通过 `Deref<Target = BasePerson>` 复用引擎通用字段，并将剧本侧身份独立保存在 `PersonOrigin` 枚举中；构造时由 `PersonOrigin` **单向**决定引擎 `PersonType`，任何剧本逻辑禁止从 `PersonType` 反推 `PersonOrigin`。

## Consequences

- 无需修改 `umaai_next/` 引擎枚举即可表达剧本专属人物语义，保持引擎边界稳定。
- 剧本内所有角色来源判定必须读取 `PersonOrigin`，防止社交人头与编成外神驹在规则上混淆。
