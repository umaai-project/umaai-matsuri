//! 程序更新：`program` 频道，整包 zip + 逐文件摘要 + Authenticode 验签。
//!
//! 清单 `<基址>/program/update.json` 声明 zip 的尺寸 / sha256 与包内逐文件摘要。
//! 判定无状态：先按 `files` 与本地逐文件比对，不下载整包；确认后下载 → 校验 →
//! 解压到临时目录 → 逐文件复核 → 校验签名指纹 → 备份替换（失败回滚）。
//! 启动器本体随包更新时把自身 `rename` 为 `.old`，拉起新进程并退出。
//!
//! 包内只含程序本体与程序资产：**不含小黑板本体、`.portable/` 与三张数据表**
//! （小黑板数据走 `ura_data`，基础表走 `ai_data`）。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use umaai_update::{FileEntry, HttpTransport, Source, Transport, hash, sha256_file};

use super::{Paint, Paths, Result, fail, platform_exe};

/// 远端频道目录名
const CHANNEL: &str = "program";
/// 清单协议版本
const PROTOCOL: u32 = 1;
/// 解压与下载的临时根（相对安装根）
const WORK_ROOT: &str = ".umaai-update";
/// 程序频道在临时根下的子目录
const TEMP_DIR: &str = "program";
/// 包内文件白名单（字典序；清单必须与它严格相等）
const PROGRAM_FILES: [&str; 8] = [
    "bin/matsuri.exe",
    "data/ai_config.toml",
    "data/constants.json",
    "data/events.json",
    "data/manifest.json",
    "data/matsuri_config.toml",
    "data/scenario_matsuri.json",
    "matsuri-ai.exe",
];
/// 允许的签名证书指纹（自签 `CN=UmaAI`）
#[cfg(windows)]
const SIGNER_THUMBPRINTS: [&str; 1] = ["327CCE93FC7AAA25CD88CADD5C7280B9FFA58E92"];
/// 更新完成后由新进程读取的标记环境变量
const UPDATED_ENV: &str = "MATSURI_UPDATED";
/// 启动器本体在包内的相对路径（最后落地）
const LAUNCHER_REL: &str = "matsuri-ai.exe";
/// 核心程序在包内的相对路径
const CORE_REL: &str = "bin/matsuri.exe";

/// 程序更新清单（`program/update.json`）
#[derive(Debug, Deserialize)]
pub(super) struct ProgramManifest {
    /// 清单协议版本
    protocol: u32,
    /// 发布版本（仅显示）
    #[serde(default)]
    version: String,
    /// 发布时间（仅显示）
    #[serde(default)]
    date: Option<String>,
    /// 整包信息
    package: Package,
    /// 包内逐文件摘要
    files: Vec<FileEntry>,
}

/// 整包信息
#[derive(Debug, Deserialize)]
struct Package {
    /// 包文件名（单段、相对路径）
    name: String,
    /// 字节数
    size: u64,
    /// sha256
    sha256: String,
}

/// 执行一次检查、确认、更新；启动器有变化时重启并退出
pub(super) fn run(paths: &Paths, paint: &Paint) -> Result<()> {
    let source = super::update::source()?;
    let transport = HttpTransport::new();

    println!();
    println!("提示：更新程序前请退出在线模式；小黑板可保持运行（本体不参与更新）。");
    println!("{}", paint.bold("检查程序更新…"));
    let manifest = fetch_manifest(&source, &transport)?;
    let pending = pending_files(&paths.install_dir, &manifest);
    if pending.is_empty() {
        println!(
            "{} 已是最新：v{}",
            paint.green("●"),
            version_text(&manifest)
        );
        return Ok(());
    }

    println!(
        "{} 待更新 v{}：{} 个文件，整包 {:.1} MB",
        paint.yellow("●"),
        version_text(&manifest),
        pending.len(),
        manifest.package.size as f64 / 1024.0 / 1024.0
    );
    for entry in &pending {
        println!("    {}", entry.path);
    }
    if !super::update::confirm()? {
        println!("已取消，未做任何更新。");
        return Ok(());
    }

    let temp = paths.install_dir.join(WORK_ROOT).join(TEMP_DIR);
    if temp.exists() {
        fs::remove_dir_all(&temp)?;
    }
    fs::create_dir_all(&temp)?;
    let zip_path = temp.join(&manifest.package.name);
    println!("{}", paint.bold("下载程序包…"));
    download(&source, &transport, &manifest.package, &zip_path)?;

    println!("{}", paint.bold("解压并校验…"));
    let extract = temp.join("extract");
    extract_package(&zip_path, &extract, &manifest)?;
    verify_signature(&extract)?;

    println!("{}", paint.bold("替换程序文件…"));
    let restart = pending.iter().any(|entry| entry.path == LAUNCHER_REL);
    if let Err(err) = install(&paths.install_dir, &extract, &manifest) {
        // 失败保留临时目录供排查（成功路径由 cleanup_after_install 清理）
        return Err(fail(format!("{err}（临时目录已保留：{}）", temp.display())));
    }
    cleanup_after_install(&paths.install_dir, restart);

    if restart {
        println!("程序已更新，正在重启启动器…");
        restart_self(&paths.install_dir)?;
        std::process::exit(0);
    }
    println!(
        "{} 程序已更新：v{}",
        paint.green("●"),
        version_text(&manifest)
    );
    println!("提示：重新进入在线模式后生效。");
    Ok(())
}

/// 启动时清理上次更新留下的 `.old` 备份与空临时目录，并在被拉起时提示已更新
pub(super) fn startup(install_dir: &Path) {
    for rel in PROGRAM_FILES {
        remove_with_retry(&old_path(&local_path(install_dir, rel)));
    }
    let _ = fs::remove_dir(install_dir.join(WORK_ROOT));
    if std::env::var(UPDATED_ENV).is_ok() {
        println!("程序已更新到当前版本。");
    }
}

/// 更新成功后清理：临时目录整个删除；非启动器的 `.old` 当场删掉。
///
/// 启动器自身正占用 `.old`（自己的镜像），交给新进程的 [`startup`] 重试删除。
fn cleanup_after_install(install_dir: &Path, restart: bool) {
    let work = install_dir.join(WORK_ROOT);
    let _ = fs::remove_dir_all(work.join(TEMP_DIR));
    let _ = fs::remove_dir(work);
    for rel in PROGRAM_FILES {
        if restart && rel == LAUNCHER_REL {
            continue;
        }
        let _ = fs::remove_file(old_path(&local_path(install_dir, rel)));
    }
}

/// 删除文件；Windows 上刚退出的进程镜像可能短暂占用，短暂重试后放弃
fn remove_with_retry(path: &Path) {
    for _ in 0..10 {
        match fs::remove_file(path) {
            Ok(()) => return,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}

/// 版本显示文本
pub(super) fn version_text(manifest: &ProgramManifest) -> String {
    match &manifest.date {
        Some(date) if !manifest.version.is_empty() => format!("{}（{date}）", manifest.version),
        Some(date) => date.clone(),
        None if manifest.version.is_empty() => "未知版本".to_string(),
        None => manifest.version.clone(),
    }
}

/// 版本号（不含日期，供头部一行等紧凑场景使用）
pub(super) fn version_label(manifest: &ProgramManifest) -> String {
    if manifest.version.is_empty() {
        "未知版本".to_string()
    } else {
        manifest.version.clone()
    }
}

/// 整包字节数（程序频道一次更新的实际下载量）
pub(super) fn package_size(manifest: &ProgramManifest) -> u64 {
    manifest.package.size
}

/// 拉取并校验清单（启动自检只走到这一步，不下载整包）
pub(super) fn fetch_manifest(
    source: &Source,
    transport: &dyn Transport,
) -> Result<ProgramManifest> {
    let url = source.manifest_url(CHANNEL);
    let mut buf = Vec::new();
    transport
        .download(&url, source.headers(), &mut buf)
        .map_err(|err| fail(format!("拉取程序清单失败：{err}")))?;
    let manifest: ProgramManifest =
        serde_json::from_slice(&buf).map_err(|err| fail(format!("解析程序清单失败：{err}")))?;
    validate(&manifest)?;
    Ok(manifest)
}

/// 校验协议、包名与文件集合（白名单必须严格相等）
fn validate(manifest: &ProgramManifest) -> Result<()> {
    if manifest.protocol != PROTOCOL {
        return Err(fail(format!(
            "程序清单协议 {} 不被支持（本程序支持 {PROTOCOL}，需更新程序）",
            manifest.protocol
        )));
    }
    let name = manifest.package.name.as_str();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(fail(format!("程序包名非法：{name}")));
    }
    if !name.ends_with(".zip") {
        return Err(fail(format!("程序包必须是 zip：{name}")));
    }
    if !hash::is_sha256_hex(&manifest.package.sha256) {
        return Err(fail("程序包 sha256 不是 64 位小写十六进制"));
    }
    let mut got: Vec<&str> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    got.sort_unstable();
    if got != PROGRAM_FILES {
        return Err(fail(format!(
            "程序清单文件集合与本地约定不一致：远端 {got:?}，只允许 {PROGRAM_FILES:?}"
        )));
    }
    for entry in &manifest.files {
        if !hash::is_sha256_hex(&entry.sha256) {
            return Err(fail(format!("{} 的 sha256 格式非法", entry.path)));
        }
    }
    Ok(())
}

/// 待更新文件（本地缺失或摘要不同）
pub(super) fn pending_files<'a>(
    install: &Path,
    manifest: &'a ProgramManifest,
) -> Vec<&'a FileEntry> {
    manifest
        .files
        .iter()
        .filter(|entry| !local_matches(&local_path(install, &entry.path), entry))
        .collect()
}

/// 本地文件是否与清单一致（大小先比，再比 sha256）
fn local_matches(path: &Path, entry: &FileEntry) -> bool {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() == entry.size => {
            sha256_file(path).is_ok_and(|digest| digest == entry.sha256)
        }
        _ => false,
    }
}

/// 包内相对路径映射到安装目录（两个 exe 走平台文件名）
fn local_path(install: &Path, rel: &str) -> PathBuf {
    match rel {
        LAUNCHER_REL => install.join(platform_exe("matsuri-ai")),
        CORE_REL => install.join("bin").join(platform_exe("matsuri")),
        _ => install.join(rel),
    }
}

/// 备份文件名：同目录 `<文件名>.old`
fn old_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.old"))
}

/// 下载整包并校验尺寸与 sha256
fn download(
    source: &Source,
    transport: &dyn Transport,
    package: &Package,
    dest: &Path,
) -> Result<()> {
    let url = source.file_url(CHANNEL, &package.name);
    let mut file = fs::File::create(dest)?;
    let written = transport
        .download(&url, source.headers(), &mut file)
        .map_err(|err| fail(format!("下载 {} 失败：{err}", package.name)))?;
    file.flush()?;
    drop(file);
    if written != package.size {
        return Err(fail(format!(
            "程序包大小不符：清单 {} 字节，实际 {written} 字节",
            package.size
        )));
    }
    let actual = sha256_file(dest)?;
    if actual != package.sha256 {
        return Err(fail(format!("程序包 sha256 不符：{actual}")));
    }
    Ok(())
}

/// 解压到临时目录：拒绝清单外条目与越界路径，逐文件复核摘要
fn extract_package(zip_path: &Path, dir: &Path, manifest: &ProgramManifest) -> Result<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|err| fail(format!("打开程序包失败：{err}")))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| fail(format!("读取程序包条目失败：{err}")))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(fail(format!("程序包含非法路径：{}", entry.name())));
        };
        let rel = name.to_string_lossy().replace('\\', "/");
        let Some(expected) = manifest.files.iter().find(|file| file.path == rel) else {
            return Err(fail(format!("程序包含清单外条目：{rel}")));
        };
        let out = dir.join(&rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut sink = fs::File::create(&out)?;
        io::copy(&mut entry, &mut sink)?;
        sink.flush()?;
        drop(sink);
        let actual = sha256_file(&out)?;
        if actual != expected.sha256 {
            return Err(fail(format!("{rel} 摘要不符：{actual}")));
        }
    }
    Ok(())
}

/// 已落地的文件（回滚用）
struct Placed {
    /// 目标路径
    target: PathBuf,
    /// 旧文件备份（原本不存在为 `None`）
    backup: Option<PathBuf>,
}

/// 备份并替换：启动器最后落地，任一失败回滚已替换项
fn install(install_dir: &Path, extract: &Path, manifest: &ProgramManifest) -> Result<()> {
    let mut order: Vec<&FileEntry> = manifest.files.iter().collect();
    order.sort_by_key(|entry| entry.path == LAUNCHER_REL);
    let mut placed: Vec<Placed> = Vec::with_capacity(order.len());
    for entry in order {
        let target = local_path(install_dir, &entry.path);
        let src = extract.join(&entry.path);
        match place_one(&src, &target) {
            Ok(backup) => placed.push(Placed { target, backup }),
            Err(err) => {
                rollback(&placed);
                return Err(err);
            }
        }
    }
    Ok(())
}

/// 单个文件：旧文件改名为 `.old`，再把新文件改名就位
fn place_one(src: &Path, target: &Path) -> Result<Option<PathBuf>> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let backup = if target.exists() {
        let backup = old_path(target);
        let _ = fs::remove_file(&backup);
        fs::rename(target, &backup)
            .map_err(|err| fail(format!("备份 {} 失败：{err}", target.display())))?;
        Some(backup)
    } else {
        None
    };
    if let Err(err) = fs::rename(src, target) {
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, target);
        }
        return Err(fail(format!("替换 {} 失败：{err}", target.display())));
    }
    Ok(backup)
}

/// 按逆序还原已替换项
fn rollback(placed: &[Placed]) {
    for item in placed.iter().rev() {
        match &item.backup {
            Some(backup) => {
                let _ = fs::remove_file(&item.target);
                let _ = fs::rename(backup, &item.target);
            }
            None => {
                let _ = fs::remove_file(&item.target);
            }
        }
    }
}

/// 拉起新启动器（带「已更新」标记），由调用方退出当前进程
fn restart_self(install_dir: &Path) -> Result<()> {
    let exe = local_path(install_dir, LAUNCHER_REL);
    Command::new(&exe)
        .current_dir(install_dir)
        .env(UPDATED_ENV, "1")
        .spawn()
        .map(|_| ())
        .map_err(|err| {
            fail(format!(
                "拉起新启动器失败：{err}（可手工运行 {}）",
                exe.display()
            ))
        })
}

/// 校验两个 exe 的 Authenticode 指纹（自签根不受信任属预期，不要求 Valid）
#[cfg(windows)]
fn verify_signature(extract: &Path) -> Result<()> {
    for rel in [LAUNCHER_REL, CORE_REL] {
        let path = extract.join(rel);
        let (status, thumbprint) = signature_of(&path)?;
        if !SIGNER_THUMBPRINTS
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&thumbprint))
        {
            return Err(fail(format!(
                "{rel} 签名指纹不在白名单：{thumbprint}（状态 {status}）"
            )));
        }
        if matches!(
            status.as_str(),
            "NotSigned" | "HashMismatch" | "NotSupportedFileFormat"
        ) {
            return Err(fail(format!("{rel} 签名校验失败：{status}")));
        }
    }
    Ok(())
}

/// 非 Windows 不支持程序更新（需要 Authenticode 验签）
#[cfg(not(windows))]
fn verify_signature(_extract: &Path) -> Result<()> {
    Err(fail("程序更新仅支持 Windows（需要 Authenticode 验签）"))
}

/// 读取签名状态与证书指纹（`Get-AuthenticodeSignature`）
#[cfg(windows)]
fn signature_of(path: &Path) -> Result<(String, String)> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let shell = Path::new(&root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let literal = path.display().to_string().replace('\'', "''");
    let script = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath '{literal}'; \
         Write-Output \"$($s.Status)|$($s.SignerCertificate.Thumbprint)\""
    );
    let out = Command::new(&shell)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|err| fail(format!("调用 PowerShell 验签失败：{err}")))?;
    if !out.status.success() {
        return Err(fail(format!("PowerShell 验签异常：{}", out.status)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or_default().trim();
    let (status, thumbprint) = line
        .split_once('|')
        .ok_or_else(|| fail(format!("验签输出无法解析：{line}")))?;
    Ok((status.trim().to_string(), thumbprint.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试临时目录：放测试二进制旁边（target 内可写），沙箱下系统临时目录不可写
    fn temp_base() -> std::path::PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .map_or_else(std::env::temp_dir, |p| p.join("tmp-tests"))
    }

    /// 造一份最小清单（files 为白名单全集）
    fn manifest(name: &str, files: &[&str]) -> ProgramManifest {
        ProgramManifest {
            protocol: PROTOCOL,
            version: "20261109".to_string(),
            date: None,
            package: Package {
                name: name.to_string(),
                size: 1,
                sha256: "0".repeat(64),
            },
            files: files
                .iter()
                .map(|path| FileEntry {
                    path: (*path).to_string(),
                    size: 1,
                    sha256: "0".repeat(64),
                })
                .collect(),
        }
    }

    /// 白名单本身必须按字典序排列（集合比较依赖它）
    #[test]
    fn program_files_sorted() {
        let mut sorted = PROGRAM_FILES;
        sorted.sort_unstable();
        assert_eq!(sorted, PROGRAM_FILES);
    }

    /// 清单文件集合严格等于白名单，多一个少一个都拒绝
    #[test]
    fn validate_rejects_extra_or_missing() {
        assert!(validate(&manifest("app.zip", &PROGRAM_FILES)).is_ok());
        let mut extra = PROGRAM_FILES.to_vec();
        extra.push("data/umaDB.json");
        assert!(validate(&manifest("app.zip", &extra)).is_err());
        assert!(validate(&manifest("app.zip", &PROGRAM_FILES[1..])).is_err());
    }

    /// 包名必须是单段 zip
    #[test]
    fn validate_rejects_bad_package_name() {
        assert!(validate(&manifest("../app.zip", &PROGRAM_FILES)).is_err());
        assert!(validate(&manifest("a/b.zip", &PROGRAM_FILES)).is_err());
        assert!(validate(&manifest("app.tar", &PROGRAM_FILES)).is_err());
    }

    /// 协议版本不符即拒绝
    #[test]
    fn validate_rejects_protocol() {
        let mut bad = manifest("app.zip", &PROGRAM_FILES);
        bad.protocol = 2;
        assert!(validate(&bad).is_err());
    }

    /// 待更新判定：缺失算待更新，写入相同内容后不再待更新
    #[test]
    fn pending_detects_missing_and_matching() {
        let dir = temp_base().join(format!("matsuri-program-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("data")).unwrap();
        let body = b"hello";
        let digest = hash::sha256_hex(body);
        let mut entry = manifest("app.zip", &PROGRAM_FILES);
        entry.files = vec![FileEntry {
            path: "data/events.json".to_string(),
            size: body.len() as u64,
            sha256: digest,
        }];
        assert_eq!(pending_files(&dir, &entry).len(), 1, "缺失应待更新");
        fs::write(dir.join("data/events.json"), body).unwrap();
        assert!(pending_files(&dir, &entry).is_empty(), "一致后应无待更新");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 备份名与目标同目录
    #[test]
    fn old_path_is_sibling() {
        assert_eq!(
            old_path(Path::new("/app/bin/matsuri.exe")),
            Path::new("/app/bin/matsuri.exe.old")
        );
    }

    /// 解压时拒绝清单外条目
    #[test]
    fn extract_rejects_entry_outside_whitelist() {
        let dir = temp_base().join(format!("matsuri-program-zip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("app.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file("evil.txt", options).unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        let manifest = manifest("app.zip", &PROGRAM_FILES);
        assert!(
            extract_package(&zip_path, &dir.join("out"), &manifest).is_err(),
            "清单外条目必须被拒绝"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 替换成功后旧文件留在同目录 `.old`
    #[test]
    fn install_keeps_old_backup() {
        let dir = temp_base().join(format!("matsuri-program-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let install_dir = dir.join("app");
        let extract = dir.join("new");
        fs::create_dir_all(install_dir.join("data")).unwrap();
        fs::create_dir_all(extract.join("data")).unwrap();
        fs::write(install_dir.join("data/events.json"), b"old").unwrap();
        fs::write(extract.join("data/events.json"), b"new").unwrap();
        let manifest = ProgramManifest {
            protocol: PROTOCOL,
            version: "v".to_string(),
            date: None,
            package: Package {
                name: "app.zip".to_string(),
                size: 1,
                sha256: "0".repeat(64),
            },
            files: vec![FileEntry {
                path: "data/events.json".to_string(),
                size: 3,
                sha256: hash::sha256_hex(b"new"),
            }],
        };
        install(&install_dir, &extract, &manifest).unwrap();
        assert_eq!(
            fs::read(install_dir.join("data/events.json")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(install_dir.join("data/events.json.old")).unwrap(),
            b"old",
            "旧文件应备份为 .old"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 成功后清理临时目录与非启动器 `.old`；启动器自身留给新进程
    #[test]
    fn cleanup_removes_temp_and_old() {
        let dir = temp_base().join(format!("matsuri-program-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let install_dir = dir.join("app");
        fs::create_dir_all(install_dir.join(WORK_ROOT).join(TEMP_DIR).join("extract")).unwrap();
        fs::write(install_dir.join(WORK_ROOT).join(TEMP_DIR).join("x"), b"x").unwrap();
        fs::create_dir_all(install_dir.join("data")).unwrap();
        let asset = local_path(&install_dir, "data/events.json");
        let launcher = local_path(&install_dir, LAUNCHER_REL);
        fs::write(old_path(&asset), b"old").unwrap();
        fs::write(old_path(&launcher), b"old").unwrap();

        cleanup_after_install(&install_dir, true);

        assert!(!install_dir.join(WORK_ROOT).exists(), "临时目录应整体删除");
        assert!(!old_path(&asset).exists(), "资产 .old 应当场删除");
        assert!(
            old_path(&launcher).exists(),
            "启动器 .old 由新进程清理，此处保留"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 中途失败回滚已替换项（第二个源文件缺失）
    #[test]
    fn install_rolls_back_on_failure() {
        let dir = temp_base().join(format!("matsuri-program-rb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let install_dir = dir.join("app");
        let extract = dir.join("new");
        fs::create_dir_all(install_dir.join("data")).unwrap();
        fs::create_dir_all(extract.join("data")).unwrap();
        fs::write(install_dir.join("data/events.json"), b"old").unwrap();
        fs::write(extract.join("data/events.json"), b"new").unwrap();
        let manifest = ProgramManifest {
            protocol: PROTOCOL,
            version: "v".to_string(),
            date: None,
            package: Package {
                name: "app.zip".to_string(),
                size: 1,
                sha256: "0".repeat(64),
            },
            files: vec![
                FileEntry {
                    path: "data/events.json".to_string(),
                    size: 3,
                    sha256: hash::sha256_hex(b"new"),
                },
                FileEntry {
                    path: "data/constants.json".to_string(),
                    size: 3,
                    sha256: hash::sha256_hex(b"new"),
                },
            ],
        };
        assert!(
            install(&install_dir, &extract, &manifest).is_err(),
            "缺源文件应失败"
        );
        assert_eq!(
            fs::read(install_dir.join("data/events.json")).unwrap(),
            b"old",
            "已替换项应回滚"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
