# MatsuriView 作为呈现层单一数据源与字段边界

`MatsuriView`（经 `MatsuriGame::view()` 输出）是手动面板、在线外壳与前端回显的**唯一状态数据源**，并显式标记为 `#[non_exhaustive]`。名称文本、动作候选/预览与建局配置刻意不进入 `MatsuriView`（分别由呈现层查静态表、决策上下文与配置层承担），以保持快照纯粹可序列化且不绑死演进边界。
