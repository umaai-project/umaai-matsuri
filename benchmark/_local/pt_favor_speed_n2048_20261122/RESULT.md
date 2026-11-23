# speed_build：搜索 PT 偏好 2.0 vs 5.0

2026-11-22。基于当前工作树重编 Release cardtest（剧本 HEAD 86bd946、引擎 HEAD ae3cab1，均含当时未提交的工作树改动）。默认目白多伯 105902、无局外切、心情盾 0；种马和继承沿用仓内默认。采用 21101 主卡组 303174/303114/303244/303224/303204/303234，不含影子卡组。相同种子 91000–91029，两档各 30 局。FlatSearch search_n=2048，沿用仓配置 UCB；教诲、训练、事件搜索开启。两份剧本配置唯一差异是 [search].pt_favor_rate，手写基策保持 1.931954。

| 终局指标 | 2.0 | 5.0 | 5.0 − 2.0 |
|---|---:|---:|---:|
| 官方评分均值 | 86029.5 | 84919.4 | -1110.1 |
| 评分样本标准差 | 1477.9 | 2073.7 | +595.8 |
| 总 PT 均值 | 12708.0 | 12988.3 | +280.3 |
| 总 PT 样本标准差 | 352.4 | 304.2 | -48.3 |
| 速度 | 2341.9 | 2340.3 | -1.6 |
| 耐力 | 1832.7 | 1798.1 | -34.7 |
| 力量 | 1785.8 | 1782.9 | -2.8 |
| 毅力 | 1605.9 | 1560.6 | -45.3 |
| 智力 | 1782.2 | 1750.5 | -31.7 |

评分配对差 95% t 区间 [-1632.3, -587.8]，5.0 胜/负 8/22；PT 配对差 [+171.1, +389.4]，5.0 胜/负 27/3。平均每多 1 PT 少 3.96 官方评分（两项均值差的比值）。五维分 -1638.4，PT 评分 +528.3，固定技能分不变。五维先逐局转显示值再平均；总 PT 含灵感折算。

## 复现

Release 重编：

```powershell
$env:CARGO_TARGET_DIR='F:/UmaAI_Active/_scratch/ctarget'
cargo build --release --offline -p matsuri-tools --bin cardtest
$dir='benchmark/_local/pt_favor_speed_n2048_20261122'
foreach ($seed in 91000..91029) {
  foreach ($favor in '2.0','5.0') {
    & "$env:CARGO_TARGET_DIR/release/cardtest.exe" run --search-n 2048 --games 1 --seed $seed --decks "$dir/speed_build.csv" --matsuri-config "$dir/favor-$favor.toml"
    if ($LASTEXITCODE -ne 0) { throw "cardtest failed: $seed/$favor" }
  }
}
```

逐局结果见同目录 results.csv。cardtest.exe SHA-256: 095D2F4CD327F16D8918597D8A1583CB8C0F40DAF09A688C130494D791A5A3B0；cardDB.json SHA-256: 2113C702A6DC1EC9281C9F92E2C4F5910D67CCB6C5C715DD5094C6490EECB152。
