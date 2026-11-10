基于 https://github.com/AlphaBoom/notion_sync_umadb 的数据库代码制作的支援卡 / 马娘数据导出脚本。

## 运行前置条件

1. 装好 Python 依赖：`pip install jsons requests`（Python ≥ 3.11，用到标准库 `tomllib`）。
2. 本机装过游戏，`master.mdb` 在游戏目录下（DMM / Steam 版一致）：
   `%USERPROFILE%\AppData\LocalLow\Cygames\umamusume\master\master.mdb`
   **不需要再把它拷到脚本目录。**
3. 翻译数据默认读 hachimi 的 `localized_data/text_data_dict.json`；该文件不存在时才会联网到
   GitHub 下载（需要能连上 GitHub）。

## 运行

在本目录执行：

```bash
python main.py                      # 用 config.toml 的默认配置
python main.py --mdb 路径\master.mdb # 临时指定库（优先级最高）
python main.py --output 临时目录     # 产物写到别处，不动 data/
python main.py --config 别的配置.toml
```

## 配置（config.toml）

所有可调数据都在 `config.toml` 里，改数据不用改代码：

| 段 | 内容 |
|---|---|
| `[paths]` | `master.mdb` 位置（留空 = 自动检测）、翻译文件与下载缓存路径 |
| `[output]` | 产物目录（默认 `../../data`，引擎消费的位置）、是否一并复制翻译数据 |
| `[support_card]` | 卡类型简称、直接取值字段、固有直并卡、无技能卡 hint 补偿、日文词条 → 下标映射 |
| `[[race_condition]]` | 自选比赛条件（等级名 / grade） |
| `[[special_races]]` | 各马娘的生涯比赛特判（追加固定比赛 / 自选区间 / 说明） |

`master.mdb` 查找顺序：`--mdb` → `[paths] master_mdb` → 环境变量 `UMAMUSUME_MASTER_MDB`
→ 游戏默认目录 → 脚本目录兜底。启动时会打印实际用的是哪份库（含大小与修改时间）。

库以**只读**方式打开（`file:...?mode=ro`），游戏运行时也不会锁库。

## 生成文件

- `data/cardDB.json`：支援卡数据（默认输出目录，见 `[output]`）
- `data/umaDB.json`：马娘数据
- `data/text_data_dict.json`：翻译数据副本（由 `[output] copy_translate` 控制，直接复制实际使用的那份）
- `text_data_dict.json`：翻译数据的联网下载缓存（仅当本地翻译文件缺失时才会写入）
