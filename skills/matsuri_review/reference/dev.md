# 开发态辅助文档（不随发布包分发）

> 本文档只存在于开发环境。发布包里删掉它；**没有本文档时，按 `SKILL.md` 与其余 reference 执行，不要尝试构建、更新或修改技能内容。**

## 1. 适用范围

- 本仓：`umaai-matsuri`；技能源目录：`skills/matsuri_review/`
- 需要：Windows、PowerShell 5.1+、Rust/cargo（本地 registry 可 `--offline`）、本仓源码
- 运行期不需要：cargo / Rust / 源码（只要 `bin/review.exe` + `data/`）

## 2. 构建与更新二进制

```powershell
powershell -NoProfile -File skills/matsuri_review/scripts/build_review_skill.ps1 -WithData
```

- 产物：`skills/matsuri_review/bin/review.exe`（`cargo build -p matsuri-tools --release --offline --bin review`，目标目录仓内 `.cargo-adv`）
- `-WithData`：把仓内 `data/` 整体复制为技能的 `data/`（数据更新后必须重跑）
- 脚本用 `$PSScriptRoot/../..` 定位本仓；**独立副本无法执行**（那里没有 cargo 工程），报错会指向错误目录

## 3. 分发

- `bin/*.exe` 与 `data/` 被 `.gitignore` 排除：只拷 git 目录会得到不可运行的技能
- 分发 = 整目录复制（`SKILL.md` + `reference/` + `bin/` + `data/`），发布包删除本文档
- 改过 review 源码后必须重建，否则跑的是旧 schema / 旧行为

## 4. 缺省数据根

`--data` 未给时的解析（实现见 `crates/matsuri-tools/src/review/mod.rs` 的 `default_data_root`）：

1. `<exe 同级>/data`
2. `<exe 上级>/data`（skill 布局：`bin/review.exe` + `data/`）
3. 仓内 `data/`（`matsuri_core::testing::data_dir()`，编译期仓根，仅开发环境）

若 exe 位于名为 `bin` 的目录且以上都没有，按第 2 条报错，避免提示构建机路径。
exe 内仍嵌构建机路径（调试信息），只影响开发态 fallback 与报错文本。

## 5. 改动后回归

```powershell
$rt = Join-Path $env:TEMP 'matsuri-rt'
<skill>\bin\review.exe open <局包> --data <skill>\data --out $rt
<skill>\bin\review.exe eval <局包> --turn <训练回合> --n 8 --data <skill>\data --out $rt
<skill>\bin\review.exe eval <局包> --turn <事件回合> --seq 0 --data <skill>\data --out $rt   # 应报「事件选项暂不比对」
<skill>\bin\review.exe infer <局包> --what advice,fusion --data <skill>\data --out $rt
<skill>\bin\review.exe curve <局包> --from 10 --to 20 --n 8 --data <skill>\data --out $rt
<skill>\bin\review.exe whatif <局包> --turn <训练回合> --swap "休息" --k 5 --data <skill>\data --out $rt
<skill>\bin\review.exe report <局包> --data <skill>\data --out $rt
<skill>\bin\review.exe show <局包> --turn <训练回合> --seq 0 --data <skill>\data   # JSON
```

便携性验证：整目录复制到仓外（带 `data`），**不传 `--data`** 跑 `open`，结果应与显式传 `--data` 一致；
再把副本 `data/` 改名，确认报错指向副本自己的 `data/`。

## 6. 其他

- 真机局包默认在 `Matsuri_AI/.portable/PluginData/SendGameStatusPlugin/`（本机示例）
- `--inherit-expect` 仅 `open` 支持，默认 306
- 文档取数片段依赖 node；无 node 时直接 Read JSON
- 构建/测试写文件需本机对仓目录有写权限；被 Windows 权限挡住时先修 ACL 再构建