# umaai-matsuri（骏大祭剧本仓）

`umaai-next` 引擎仓的「朋、史解き ―午式駿大祭―」（**駿大祭編** / scenarioId **15**）独立实现仓，提供**离线模拟**、**在线帧决策**、**测卡 / 复盘工具**与 **Windows 发布启动器**。

## 1. 获取与准备

```powershell
git clone --recursive https://github.com/umaai-project/umaai-matsuri.git
git submodule update --init --recursive   # 已有仓库补拉 umaai_next 子模块
```

## 2. 快速开始

```powershell
# 离线跑局（默认搜索档；手写档加 $env:MATSURI_SEARCH_N=0）
cargo run --release --bin matsuri --
cargo run --release --bin matsuri -- random
cargo run --release --bin matsuri -- manual
cargo run --release --bin matsuri -- matsuri --explain   # 仅手写档

# 在线：真机帧监听 / 回放
cargo run --release --bin matsuri -- online --root <游戏根目录>
cargo run --release --bin matsuri -- online --replay <帧目录> --trainer search --json

# 查看完整子命令与环境变量
cargo run --release --bin matsuri -- --help
cargo run --release --bin matsuri -- online --help

# 测卡 / 复盘工具（属于 matsuri-tools）
cargo run --release --bin cardtest -- run --decks benchmark/decks/measured.csv --ref default_21101 --games 3000 --seed 82000
cargo run --release --bin review -- open <game{id}.zip> --out <输出目录>
```

## 3. 开发验收

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --offline
cargo test --workspace --release --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
```

测试分面与基线说明见 [`docs/tests_overview.md`](docs/tests_overview.md)，性能分析见 [`docs/perf_profiling.md`](docs/perf_profiling.md)。

## 4. 深入文档

| 文档 | 用途 |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Crate 拓扑、模块落点与架构约束 |
| [`GLOSSARY.md`](GLOSSARY.md) | 领域术语与官方译名 |
| [`docs/README.md`](docs/README.md) | 剧本机制、开发计划、复盘与工具文档索引 |

## 5. 许可证

本仓以 **CC BY-NC-SA 4.0**（署名–非商业性使用–相同方式共享）发布，全文见 [`LICENSE`](LICENSE)（Windows 发布包随附同份许可证）。
