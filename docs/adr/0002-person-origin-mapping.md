# MatsuriPerson 委托与 PersonOrigin 单向映射

`MatsuriPerson` 通过 `Deref<Target = BasePerson>` 复用引擎通用字段，剧本侧身份（支援卡 / 友人 / 编成外神驹 / 理事长 / 记者）独立保存在 `PersonOrigin` 枚举中，并在构造时**单向**决定引擎 `PersonType`；任何剧本逻辑禁止从 `PersonType` 反推 `PersonOrigin`。这避免了扩展引擎 `PersonType` 带来的跨仓契约破坏，同时阻止社交人头与编成外神驹在规则判定中发生语义混淆。
