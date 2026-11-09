#!/usr/bin/env python3
"""组装 Windows 发布包（便携目录 + 可选 zip）：只拷贝、不编译、不删除任何东西。

用法：
    python3 scripts/package_launcher.py
    python3 scripts/package_launcher.py --zip
    python3 scripts/package_launcher.py --sign --program
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
- `--sign` 用 Authenticode 签名两个 exe；证书路径与口令只从环境变量读取
  （UMA_SIGN_PFX / UMA_SIGN_PFX_PASSWORD，可选 UMA_SIGNTOOL / UMA_SIGN_TIMESTAMP_URL /
  UMA_SIGN_THUMBPRINT），不写入仓库。
- `--program` 生成程序更新包 dist/program/matsuri-app-<版本>.zip 与 update.json，
  只含程序本体与程序资产，不含小黑板本体、.portable 与三张基础表。
- `--zip` 把整个发布目录打成带口令的 `Matsuri_AI-<Cargo 版本>.zip`（AES-256，文件名不加密）；
  口令只从环境变量 UMA_ZIP_PASSWORD 读取，7-Zip 命令行经 UMA_7Z 或常见安装位置定位。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import tomllib
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

# 程序更新包内的文件（清单与启动器 PROGRAM_FILES 必须逐字一致）
PROGRAM_FILES = (
    "bin/matsuri.exe",
    "data/ai_config.toml",
    "data/constants.json",
    "data/events.json",
    "data/manifest.json",
    "data/matsuri_config.toml",
    "data/scenario_matsuri.json",
    "matsuri-ai.exe",
)

# 需要 Authenticode 签名的可执行文件
SIGN_EXES = ("matsuri-ai.exe", "bin/matsuri.exe")

# 7-Zip 命令行候选位置（先看 UMA_7Z 与 PATH，再逐个尝试）
SEVENZIP_CANDIDATES = (
    r"C:\Program Files\7-Zip\7z.exe",
    r"C:\Program Files (x86)\7-Zip\7z.exe",
)


def sha256(path: Path) -> str:
    """计算文件 sha256（小写十六进制）。"""
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def signature_of(path: Path) -> tuple[str, str]:
    """用 Get-AuthenticodeSignature 读签名状态与证书指纹。"""
    script = (
        "$s = Get-AuthenticodeSignature -LiteralPath '{}'; "
        'Write-Output "$($s.Status)|$($s.SignerCertificate.Thumbprint)"'
    ).format(str(path).replace("'", "''"))
    result = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True,
        text=True,
        check=True,
    )
    status, _, thumbprint = result.stdout.strip().partition("|")
    return status.strip(), thumbprint.strip()


def sign(out: Path) -> None:
    """签名两个 exe；证书与口令只从环境变量读取。"""
    pfx = os.environ.get("UMA_SIGN_PFX")
    password = os.environ.get("UMA_SIGN_PFX_PASSWORD")
    if not pfx or not password:
        raise RuntimeError("签名需要环境变量 UMA_SIGN_PFX 与 UMA_SIGN_PFX_PASSWORD")
    signtool = os.environ.get("UMA_SIGNTOOL", "signtool")
    timestamp = os.environ.get("UMA_SIGN_TIMESTAMP_URL", "http://timestamp.digicert.com")
    for rel in SIGN_EXES:
        subprocess.run(
            [
                signtool,
                "sign",
                "/fd", "SHA256",
                "/td", "SHA256",
                "/tr", timestamp,
                "/f", pfx,
                "/p", password,
                str(out / rel),
            ],
            check=True,
        )
        print(f"  已签名 {rel}")

    expected = os.environ.get("UMA_SIGN_THUMBPRINT", "")
    for rel in SIGN_EXES:
        status, thumbprint = signature_of(out / rel)
        print(f"  验签 {rel}: 状态 {status}，指纹 {thumbprint}")
        if not thumbprint:
            raise RuntimeError(f"{rel} 缺少签名证书")
        if expected and thumbprint.upper() != expected.upper():
            raise RuntimeError(f"{rel} 指纹 {thumbprint} 与期望 {expected} 不一致")


def cargo_version() -> str:
    """读取 workspace 版本（`Cargo.toml` 的 `[workspace.package] version`）。"""
    data = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))
    version = data.get("workspace", {}).get("package", {}).get("version")
    if not version:
        raise RuntimeError("Cargo.toml 缺少 [workspace.package] version")
    return str(version)


def build_program(out: Path, dest: Path, version: str) -> None:
    """按 umaai-update 风格清单打包程序更新包（zip + update.json）。"""
    dest.mkdir(parents=True, exist_ok=True)
    name = f"matsuri-app-{version}.zip"
    archive = dest / name
    files: list[dict[str, object]] = []
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
        for rel in PROGRAM_FILES:
            src = out / rel
            zf.write(src, rel)
            files.append(
                {"path": rel, "size": src.stat().st_size, "sha256": sha256(src)}
            )
    manifest = {
        "protocol": 1,
        "version": version,
        "date": time.strftime("%Y-%m-%d %H:%M:%S"),
        "package": {
            "name": name,
            "size": archive.stat().st_size,
            "sha256": sha256(archive),
        },
        "files": files,
    }
    (dest / "update.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"已生成程序包：{archive}（{len(files)} 个文件）")
    print(f"已生成清单：{dest / 'update.json'}")
    print(f"上传：coscmd upload -rs \"{dest}\" /uma/program/")


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


def find_sevenzip() -> str:
    """定位 7-Zip 命令行：UMA_7Z > PATH > 常见安装位置。"""
    env = os.environ.get("UMA_7Z")
    if env:
        if not Path(env).is_file():
            raise RuntimeError(f"UMA_7Z 指向的文件不存在：{env}")
        return env
    found = shutil.which("7z")
    if found:
        return found
    for candidate in SEVENZIP_CANDIDATES:
        if Path(candidate).is_file():
            return candidate
    raise RuntimeError("找不到 7z 命令行；请安装 7-Zip 或设置 UMA_7Z")


def make_zip(out: Path, version: str) -> Path:
    """把发布目录打成带口令的 `Matsuri_AI-<版本>.zip`（AES-256；zip 标准不支持加密文件名）。"""
    password = os.environ.get("UMA_ZIP_PASSWORD")
    if not password:
        raise RuntimeError("加密 zip 需要环境变量 UMA_ZIP_PASSWORD")
    seven = find_sevenzip()
    archive = out.with_name(f"Matsuri_AI-{version}.zip")
    archive.unlink(missing_ok=True)
    print(f"正在打包：{archive}")
    result = subprocess.run(
        [
            seven, "a", "-tzip", "-mx=5", "-mem=AES256",
            f"-p{password}",
            str(archive),
            str(out) + os.sep + "*",
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"7z 打包失败（退出码 {result.returncode}）：{result.stderr.strip()}"
        )
    check = subprocess.run(
        [seven, "t", f"-p{password}", str(archive)],
        capture_output=True,
        text=True,
    )
    if check.returncode != 0:
        raise RuntimeError(f"zip 自检失败：{check.stderr.strip()}")
    size = archive.stat().st_size / 1024 / 1024
    print(f"已打包：{archive}（{size:.1f} MB，AES-256 口令保护）")
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
    parser.add_argument(
        "--zip",
        action="store_true",
        help="打成带口令的 Matsuri_AI-<版本>.zip（AES-256，口令取 UMA_ZIP_PASSWORD）",
    )
    parser.add_argument(
        "--sign",
        action="store_true",
        help="用 Authenticode 签名两个 exe（证书与口令取环境变量，不入库）",
    )
    parser.add_argument(
        "--program",
        action="store_true",
        help="生成程序更新包 dist/program/（zip + update.json）",
    )
    parser.add_argument(
        "--program-version",
        default=None,
        help="程序更新包版本号（默认取 Cargo.toml 的 workspace 版本）",
    )
    parser.add_argument(
        "--program-out",
        type=Path,
        default=REPO / "dist" / "program",
        help="程序更新包输出目录",
    )
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

    if args.sign:
        try:
            sign(out)
        except (OSError, RuntimeError, subprocess.CalledProcessError) as err:
            print(f"签名失败：{err}", file=sys.stderr)
            return 1
    elif args.program:
        print("警告：未加 --sign，程序更新包内的 exe 没有签名，客户端会拒绝安装")

    if args.program:
        try:
            version = args.program_version or cargo_version()
            build_program(out, args.program_out.resolve(), version)
        except (OSError, RuntimeError) as err:
            print(f"生成程序包失败：{err}", file=sys.stderr)
            return 1

    if args.zip:
        try:
            make_zip(out, cargo_version())
        except (OSError, RuntimeError) as err:
            print(f"打包失败：{err}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
