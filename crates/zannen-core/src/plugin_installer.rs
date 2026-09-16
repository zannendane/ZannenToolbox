//! 插件包安装器：`.znplugin`（zip）校验 → staging 解压 → 原子替换。
//!
//! ## 包格式
//!
//! ```text
//! zannen.debugger-0.2.0.znplugin（zip）
//! ├── plugin.toml                       # 必需；id/version/api 校验
//! ├── libzannen_debugger.dylib          # 各平台动态库（至少含当前平台）
//! ├── zannen_debugger.dll
//! └── frontend/dist/…                   # 前端声明存在时必需
//! ```
//!
//! ## 安全防线
//!
//! 1. sha256 与更新清单声明值比对（提供时）；
//! 2. ed25519 签名校验（对包文件完整字节验签；未签名包默认拒绝，须显式 `allow_unsigned`）；
//! 3. zip 完整性与路径净化（拒绝 `..` 与绝对路径）；
//! 4. `plugin.toml` 的 `api` 不得高于宿主 ABI 版本；
//! 5. id 必须与目标目录一致（防串包）；
//! 6. staging → 备份 → 替换，任何一步失败回滚，旧版本不受影响。
//!
//! 验签公钥固定在壳内（[`PLUGIN_SIGNING_PUBKEY_HEX`]），
//! 签名工具与密钥管理见 `scripts/sign-plugin.mjs`，信任模型见 docs/PACKAGING-UPDATES.md。

use std::fs;
use std::io::{Read, Seek};
use std::path::{Component, Path};

use ed25519_dalek::VerifyingKey;
use semver::Version;
use sha2::Digest;
use zannen_plugin_api::{PluginManifest, ZANNEN_ABI_VERSION};

/// 插件包发布公钥（hex，32 字节 ed25519）。
/// 私钥由签名机保管（`~/.zannen/keys/plugin-ed25519.pem`），
/// 轮换密钥须同步此处并随壳发布新版本。
pub const PLUGIN_SIGNING_PUBKEY_HEX: &str =
    "0bc4c4401577c404b8ab8e3a183241ab886a2806e95e5852ae230da1a36e3050";

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("[E4101] IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("[E4102] zip parse failed: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("[E4103] sha256 mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("[E4104] plugin.toml missing or invalid in package: {0}")]
    BadManifest(String),
    #[error("[E4105] plugin api={got} newer than host ABI {max}; upgrade ZannenToolbox first")]
    AbiTooNew { got: u32, max: u32 },
    #[error("[E4106] illegal plugin id `{0}`")]
    BadId(String),
    #[error("[E4107] package id `{package_id}` does not match target `{target_id}`")]
    IdMismatch {
        package_id: String,
        target_id: String,
    },
    #[error("[E4108] unsafe path in package: {0}")]
    UnsafePath(String),
    #[error("[E4109] missing library for current platform: {0}")]
    MissingLibrary(String),
    #[error("[E4110] plugin package is unsigned; explicitly allow unsigned installs (allow_unsigned) only if the source is trusted")]
    Unsigned,
    #[error("[E4111] plugin package signature verification failed: {0}")]
    BadSignature(String),
}

/// 安装结果报告。
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallReport {
    pub id: String,
    pub version: String,
    pub previous_version: Option<String>,
    pub warnings: Vec<String>,
}

pub fn sha256_hex(path: &Path) -> Result<String, InstallError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// hex 解码（忽略首尾空白），出错返回可读描述。
fn unhex(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err("hex length is odd".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| format!("invalid hex character: {}", &s[i..i + 2]))
        })
        .collect()
}

/// 解析壳内固定的验签公钥。常量为构建期固化值，解析失败属打包错误，直接 panic。
fn embedded_verifying_key() -> VerifyingKey {
    let bytes = unhex(PLUGIN_SIGNING_PUBKEY_HEX).expect("embedded public key hex is invalid");
    let arr: [u8; 32] = bytes
        .try_into()
        .expect("embedded public key must be 32 bytes");
    VerifyingKey::from_bytes(&arr).expect("embedded public key is invalid")
}

/// 校验包并返回其清单（不做任何写操作）。
pub fn inspect_archive(
    archive: &mut zip::ZipArchive<impl Read + Seek>,
    target_id: &str,
) -> Result<PluginManifest, InstallError> {
    let text = {
        let mut manifest_file = archive
            .by_name("plugin.toml")
            .map_err(|e| InstallError::BadManifest(e.to_string()))?;
        let mut text = String::new();
        manifest_file
            .read_to_string(&mut text)
            .map_err(|e| InstallError::BadManifest(e.to_string()))?;
        text
    };
    let manifest: PluginManifest =
        toml::from_str(&text).map_err(|e| InstallError::BadManifest(e.to_string()))?;

    if manifest.api > ZANNEN_ABI_VERSION {
        return Err(InstallError::AbiTooNew {
            got: manifest.api,
            max: ZANNEN_ABI_VERSION,
        });
    }
    // id 形如 "zannen.debugger"（允许点号），只拒绝路径分隔符与空值
    if manifest.id.is_empty() || manifest.id.contains('/') || manifest.id.contains('\\') {
        return Err(InstallError::BadId(manifest.id));
    }
    if manifest.id != target_id {
        return Err(InstallError::IdMismatch {
            package_id: manifest.id,
            target_id: target_id.to_string(),
        });
    }

    // 当前平台动态库必须存在
    if let Some(backend) = &manifest.backend {
        let lib = crate::plugin_manager::platform_lib_name(&backend.name);
        let mut found = false;
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                if entry.name() == lib {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(InstallError::MissingLibrary(lib));
        }
    }
    Ok(manifest)
}

/// 安装插件包：校验（hash → 签名 → manifest/ABI）→ staging → 替换。
/// `plugins_root` 为插件根目录（如 src-tauri/plugins）。
///
/// - `signature`：包文件字节的 ed25519 签名（hex），由 `scripts/sign-plugin.mjs` 产出；
///   提供时用壳内固定公钥 [`PLUGIN_SIGNING_PUBKEY_HEX`] 验签，失败即拒绝；
/// - `signature` 为 `None` 时，`allow_unsigned` 为 true 才允许安装（报告中带警告）。
pub fn install_archive(
    plugins_root: &Path,
    target_id: &str,
    archive_path: &Path,
    expected_sha256: Option<&str>,
    signature: Option<&str>,
    allow_unsigned: bool,
) -> Result<InstallReport, InstallError> {
    let key = embedded_verifying_key();
    install_archive_with_key(
        plugins_root,
        target_id,
        archive_path,
        expected_sha256,
        signature,
        allow_unsigned,
        &key,
    )
}

/// 同 [`install_archive`]，但验签公钥由调用方注入（测试钩子；生产路径用壳内固定公钥）。
pub(crate) fn install_archive_with_key(
    plugins_root: &Path,
    target_id: &str,
    archive_path: &Path,
    expected_sha256: Option<&str>,
    signature: Option<&str>,
    allow_unsigned: bool,
    verifying_key: &VerifyingKey,
) -> Result<InstallReport, InstallError> {
    if let Some(expected) = expected_sha256 {
        let actual = sha256_hex(archive_path)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(InstallError::HashMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
    }

    let mut warnings = Vec::new();

    // 签名校验：对包文件完整字节验签（verify_strict 拒绝可变形签名）
    match signature {
        Some(sig_hex) => {
            let sig_bytes = unhex(sig_hex).map_err(InstallError::BadSignature)?;
            let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| {
                InstallError::BadSignature("signature must be 64 bytes".to_string())
            })?;
            let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
            let bytes = fs::read(archive_path)?;
            verifying_key.verify_strict(&bytes, &sig).map_err(|_| {
                InstallError::BadSignature(
                    "signature does not match package bytes or release public key".to_string(),
                )
            })?;
        }
        None => {
            if allow_unsigned {
                warnings.push("package is unsigned; signature check skipped by explicit opt-in - verify the source is trusted".to_string());
            } else {
                return Err(InstallError::Unsigned);
            }
        }
    }

    let file = fs::File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let manifest = inspect_archive(&mut zip, target_id)?;

    // 已装版本（降级提示）
    let target_dir = plugins_root.join(target_id);
    let previous_version = read_installed_version(&target_dir);
    if let Some(prev) = &previous_version {
        if let (Ok(old), Ok(new)) = (Version::parse(prev), Version::parse(&manifest.version)) {
            if new < old {
                warnings.push(format!("version downgrade: {old} -> {new}"));
            }
        }
    }
    // 前端入口存在性提示
    if let Some(frontend) = &manifest.frontend {
        let mut has_entry = false;
        for i in 0..zip.len() {
            if let Ok(entry) = zip.by_index(i) {
                if entry.name() == frontend.entry {
                    has_entry = true;
                    break;
                }
            }
        }
        if !has_entry {
            warnings.push(format!(
                "frontend entry missing in package: {}",
                frontend.entry
            ));
        }
    }

    // staging 解压
    let staging = plugins_root.join(format!(".staging-{target_id}"));
    let backup = plugins_root.join(format!(".backup-{target_id}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let extract_result = (|| -> Result<(), InstallError> {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            let rel = Path::new(&name);
            if rel
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
            {
                return Err(InstallError::UnsafePath(name));
            }
            let out = staging.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&out)?;
            } else {
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut w = fs::File::create(&out)?;
                std::io::copy(&mut entry, &mut w)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = extract_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    // 原子替换（同卷 rename）：target → backup，staging → target，失败回滚。
    if target_dir.exists() {
        let _ = fs::remove_dir_all(&backup);
        fs::rename(&target_dir, &backup)?;
    }
    if let Err(e) = fs::rename(&staging, &target_dir) {
        // 回滚
        if backup.exists() {
            let _ = fs::rename(&backup, &target_dir);
        }
        return Err(InstallError::Io(e));
    }
    let _ = fs::remove_dir_all(&backup);

    log::info!(
        "plugin {} installed: {} (prev: {:?})",
        target_id,
        manifest.version,
        previous_version
    );
    Ok(InstallReport {
        id: target_id.to_string(),
        version: manifest.version,
        previous_version,
        warnings,
    })
}

fn read_installed_version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("plugin.toml")).ok()?;
    let manifest: PluginManifest = toml::from_str(&text).ok()?;
    Some(manifest.version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use std::io::Write;
    use std::path::PathBuf;

    /// 测试用固定密钥对（固定种子 → 确定性，无需随机数依赖）。
    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    /// 用测试私钥对包文件字节签名，返回 hex（与 scripts/sign-plugin.mjs 的约定一致）。
    fn sign_package(path: &Path, key: &SigningKey) -> String {
        use ed25519_dalek::Signer;
        hex(&key.sign(&fs::read(path).unwrap()).to_bytes())
    }

    /// 构造一个内存 zip 插件包。
    fn build_test_package(
        dir: &Path,
        id: &str,
        version: &str,
        api: u32,
        with_lib: bool,
    ) -> PathBuf {
        let path = dir.join(format!("{id}-{version}.znplugin"));
        let file = fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zip.start_file("plugin.toml", opts).unwrap();
        write!(
            zip,
            "id = \"{id}\"\nname = \"Test\"\nversion = \"{version}\"\napi = {api}\n\n[backend]\nname = \"test_plugin\"\n"
        )
        .unwrap();
        if with_lib {
            let lib = crate::plugin_manager::platform_lib_name("test_plugin");
            zip.start_file(lib, opts).unwrap();
            zip.write_all(b"\x00fake").unwrap();
        }
        zip.start_file("frontend/dist/index.js", opts).unwrap();
        zip.write_all(b"export default { routes: {} }").unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn install_happy_path_and_upgrade() {
        let tmp = std::env::temp_dir().join(format!("zn-install-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let root = tmp.join("plugins");
        fs::create_dir_all(&root).unwrap();
        let key = test_signing_key();
        let vk = key.verifying_key();

        // 装 v1（合法签名）
        let v1 = build_test_package(&tmp, "test.plugin", "1.0.0", 1, true);
        let sig1 = sign_package(&v1, &key);
        let r1 = install_archive_with_key(&root, "test.plugin", &v1, None, Some(&sig1), false, &vk)
            .unwrap();
        assert_eq!(r1.version, "1.0.0");
        assert!(r1.previous_version.is_none());
        assert!(root.join("test.plugin/plugin.toml").exists());

        // 升级到 v2，sha256 与签名校验均通过
        let v2 = build_test_package(&tmp, "test.plugin", "1.1.0", 1, true);
        let hash = sha256_hex(&v2).unwrap();
        let sig2 = sign_package(&v2, &key);
        let r2 = install_archive_with_key(
            &root,
            "test.plugin",
            &v2,
            Some(&hash),
            Some(&sig2),
            false,
            &vk,
        )
        .unwrap();
        assert_eq!(r2.previous_version.as_deref(), Some("1.0.0"));
        assert_eq!(r2.version, "1.1.0");
        assert!(r2.warnings.is_empty());

        // 降级警告
        let r3 = install_archive_with_key(&root, "test.plugin", &v1, None, Some(&sig1), false, &vk)
            .unwrap();
        assert!(r3.warnings.iter().any(|w| w.contains("downgrade")));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_bad_hash_abi_id() {
        let tmp = std::env::temp_dir().join(format!("zn-install-test2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let root = tmp.join("plugins");
        fs::create_dir_all(&root).unwrap();
        let key = test_signing_key();
        let vk = key.verifying_key();

        let pkg = build_test_package(&tmp, "test.plugin", "1.0.0", 1, true);
        let sig = sign_package(&pkg, &key);

        // 坏 hash（hash 校验先于签名校验）
        let err = install_archive_with_key(
            &root,
            "test.plugin",
            &pkg,
            Some("deadbeef"),
            Some(&sig),
            false,
            &vk,
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::HashMismatch { .. }));

        // ABI 过新
        let new_api = build_test_package(&tmp, "test.plugin", "2.0.0", 999, true);
        let sig_new_api = sign_package(&new_api, &key);
        let err = install_archive_with_key(
            &root,
            "test.plugin",
            &new_api,
            None,
            Some(&sig_new_api),
            false,
            &vk,
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::AbiTooNew { .. }));

        // id 不符
        let err =
            install_archive_with_key(&root, "other.plugin", &pkg, None, Some(&sig), false, &vk)
                .unwrap_err();
        assert!(matches!(err, InstallError::IdMismatch { .. }));

        // 缺动态库
        let no_lib = build_test_package(&tmp, "test.plugin", "1.0.1", 1, false);
        let sig_no_lib = sign_package(&no_lib, &key);
        let err = install_archive_with_key(
            &root,
            "test.plugin",
            &no_lib,
            None,
            Some(&sig_no_lib),
            false,
            &vk,
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::MissingLibrary(_)));

        // 全部拒绝后目标目录不应残留
        assert!(!root.join("test.plugin").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_tampered_signature() {
        let tmp = std::env::temp_dir().join(format!("zn-install-test3-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let root = tmp.join("plugins");
        fs::create_dir_all(&root).unwrap();
        let key = test_signing_key();
        let vk = key.verifying_key();

        let pkg = build_test_package(&tmp, "test.plugin", "1.0.0", 1, true);

        // 别的包字节产生的签名 → 与包内容不匹配
        let other = build_test_package(&tmp, "test.plugin", "9.9.9", 1, true);
        let sig_other = sign_package(&other, &key);
        let err = install_archive_with_key(
            &root,
            "test.plugin",
            &pkg,
            None,
            Some(&sig_other),
            false,
            &vk,
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::BadSignature(_)));

        // 篡改签名本身（翻转首字节）
        let mut sig = sign_package(&pkg, &key);
        sig.replace_range(0..2, if sig.starts_with("00") { "01" } else { "00" });
        let err =
            install_archive_with_key(&root, "test.plugin", &pkg, None, Some(&sig), false, &vk)
                .unwrap_err();
        assert!(matches!(err, InstallError::BadSignature(_)));

        // 别的密钥签的名 → 与验签公钥不匹配
        let rogue = SigningKey::from_bytes(&[9u8; 32]);
        let sig_rogue = sign_package(&pkg, &rogue);
        let err = install_archive_with_key(
            &root,
            "test.plugin",
            &pkg,
            None,
            Some(&sig_rogue),
            false,
            &vk,
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::BadSignature(_)));

        // 非法 hex / 长度不符
        let err =
            install_archive_with_key(&root, "test.plugin", &pkg, None, Some("zz"), false, &vk)
                .unwrap_err();
        assert!(matches!(err, InstallError::BadSignature(_)));
        let err =
            install_archive_with_key(&root, "test.plugin", &pkg, None, Some("abcd"), false, &vk)
                .unwrap_err();
        assert!(matches!(err, InstallError::BadSignature(_)));

        // 全部拒绝后目标目录不应残留
        assert!(!root.join("test.plugin").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unsigned_policy() {
        let tmp = std::env::temp_dir().join(format!("zn-install-test4-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let root = tmp.join("plugins");
        fs::create_dir_all(&root).unwrap();
        let key = test_signing_key();
        let vk = key.verifying_key();

        let pkg = build_test_package(&tmp, "test.plugin", "1.0.0", 1, true);

        // 默认拒绝未签名包
        let err = install_archive_with_key(&root, "test.plugin", &pkg, None, None, false, &vk)
            .unwrap_err();
        assert!(matches!(err, InstallError::Unsigned));
        assert!(!root.join("test.plugin").exists());

        // 显式允许 → 安装通过且带中文警告
        let r =
            install_archive_with_key(&root, "test.plugin", &pkg, None, None, true, &vk).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("unsigned")));
        assert!(root.join("test.plugin/plugin.toml").exists());

        let _ = fs::remove_dir_all(&tmp);
    }
}
