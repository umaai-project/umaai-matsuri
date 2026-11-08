#!/usr/bin/env python3
"""组装 Windows 发布包（便携目录 + 可选 zip）：只拷贝、不编译、不删除任何东西。

用法：
    python3 scripts/package_launcher.py
    python3 scripts/package_launcher.py --zip
    python3 scripts/package_launcher.py --release-dir target/release --out dist/matsuri-ai

约定：
- 产物布局：matsuri-ai.exe / bin/matsuri.exe / data/<九个运行时文件> / LICENSE（仓根许可证）。
- 附加资源默认取 dist/extra/（被 /dist/ 忽略）：**整个目录递归**拷到发布包根目录，
  保留相对路径与空目录（.portable/PluginData/SendGameStatusPlugin 必须是目录，
  否则核心 CLI 的 FileChannel::open 会报「小黑板数据目录不存在」）。
- 附加资源顶层若与 matsuri-ai.exe / bin / data 同名则跳过，避免覆盖核心产物。
- 不生成配置文件、不建 logs/；也不复用开发机上的任何路径。
- 默认不跑 cargo（会抢 target 锁）；构建请自行执行 cargo build --release。
- 输出目录已存在时只覆盖同名文件，不清理其它文件（零删除）。
"""

from __future__ import annotations

import argparse
import os
import shutil
import sys
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# 核心 CLI 运行时读取的全部数据文件：manifest + 五张静态表 + 两份配置 + 剧本资产。
DATA_FILES = (
    "ai_config.toml",
    "matsuri_config.toml",
    "scenario_matsuri.json",
    "manifest.json",
    "umaDB.json",
    "cardDB.json",
    "text_data_dict.json",
    "events.json",
    "constants.json",
)

# 随包分发的许可证（仓根唯一真源；CC BY-NC-SA 4.0）
LICENSE_FILE = "LICENSE"


def exe_name(stem: str) -> str:
    """可执行文件文件名（Windows 带 .exe）。"""
    return f"{stem}.exe" if os.name == "nt" else stem


def collect(release_dir: Path, out: Path, extra: Path) -> list[str]:
    """校验并拷贝白名单文件与附加资源，返回提示 / 告警信息。"""
    launcher = release_dir / exe_name("matsuri-ai")
    core = release_dir / exe_name("matsuri")
    missing = [path for path in (launcher, core) if not path.is_file()]
    if missing:
        raise FileNotFoundError(
            "缺少 release 产物：" + ", ".join(str(path) for path in missing)
        )

    data_src = REPO / "data"
    missing_data = [name for name in DATA_FILES if not (data_src / name).is_file()]
    if missing_data:
        raise FileNotFoundError("缺少运行时数据文件：" + ", ".join(missing_data))
    license_src = REPO / LICENSE_FILE
    if not license_src.is_file():
        raise FileNotFoundError(f"缺少许可证文件：{license_src}")

    out.mkdir(parents=True, exist_ok=True)
    (out / "bin").mkdir(exist_ok=True)
    (out / "data").mkdir(exist_ok=True)

    shutil.copy2(launcher, out / launcher.name)
    shutil.copy2(core, out / "bin" / core.name)
    shutil.copy2(license_src, out / LICENSE_FILE)
    for name in DATA_FILES:
        shutil.copy2(data_src / name, out / "data" / name)

    messages: list[str] = []
    reserved = {launcher.name, "bin", "data"}
    extra_names: list[str] = []
    if not extra.is_dir():
        messages.append(f"提示：未找到附加资源目录，已跳过（{extra}）")
    else:
        extra_names = [path.name for path in sorted(extra.iterdir())]
        files = 0
        dirs = 0
        total = 0
        skipped: set[str] = set()
        for path in sorted(extra.rglob("*")):
            rel = path.relative_to(extra)
            if rel.parts[0] in reserved:
                skipped.add(rel.parts[0])
                continue
            target = out / rel
            if path.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                dirs += 1
            elif path.is_file():
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
                files += 1
                total += path.stat().st_size
        for name in sorted(skipped):
            messages.append(
                f"警告：附加资源顶层条目与发布包保留项冲突，已跳过（{name}）"
            )
        messages.append(
            f"提示：已递归拷贝附加资源 {files} 个文件 / {dirs} 个目录，"
            f"共 {total / 1024 / 1024:.1f} MB"
        )

    if extra.is_dir() and not (extra / exe_name("UmamusumeResponseAnalyzer")).is_file():
        messages.append(
            "提示：附加资源缺少 UmamusumeResponseAnalyzer.exe，"
            "启动器「启动小黑板」将提示程序缺失"
        )

    expected = {launcher.name, "bin", "data", LICENSE_FILE, *extra_names}
    messages += [
        f"警告：输出目录存在非白名单条目（未删除）：{path.relative_to(out)}"
        for path in out.iterdir()
        if path.name not in expected
    ]
    return messages


def make_zip(out: Path) -> Path:
    """打包整个目录；目录条目也写入，保证解压后空目录仍然存在。"""
    archive = out.with_name(out.name + ".zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(out.rglob("*")):
            zf.write(path, path.relative_to(out.parent))
    return archive


def main() -> int:
    """解析参数、组装发布包；产物缺失时报错退出，不生成残包。"""
    parser = argparse.ArgumentParser(
        description="组装骏大祭 AI Windows 发布包（不编译、不删除）"
    )
    parser.add_argument(
        "--release-dir", type=Path, default=REPO / "target" / "release"
    )
    parser.add_argument("--out", type=Path, default=REPO / "dist" / "matsuri-ai")
    parser.add_argument(
        "--extra",
        type=Path,
        default=REPO / "dist" / "extra",
        help="附加资源目录，递归拷到发布包根目录（保留空目录）；不存在则跳过",
    )
    parser.add_argument("--zip", action="store_true", help="额外产出同名 zip")
    args = parser.parse_args()

    out = args.out.resolve()
    try:
        messages = collect(args.release_dir.resolve(), out, args.extra.resolve())
    except OSError as err:
        print(f"打包失败：{err}", file=sys.stderr)
        return 1

    print(f"已组装：{out}")
    for message in messages:
        print(message)
    if args.zip:
        print(f"已打包：{make_zip(out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
