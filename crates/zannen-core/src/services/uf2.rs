//! UF2 固件服务：镜像解析/校验、Bootloader MSC 卷检测、刷写。
//!
//! UF2 块格式（512 字节）：
//! ```text
//! 0   u32  magicStart0 = 0x0A324655
//! 4   u32  magicStart1 = 0x9E5D5157
//! 8   u32  flags（bit 0x2000 = 携带 familyID）
//! 12  u32  targetAddr
//! 16  u32  payloadSize（≤ 476）
//! 20  u32  blockNo
//! 24  u32  numBlocks
//! 28  u32  familyId（flags 携带时有效）
//! 32  payload ...
//! 508 u32  magicEnd = 0x0AB16F30
//! ```
//!
//! 参考：Microsoft UF2 规范。nRF52 常用 Adafruit UF2 bootloader
//! （familyID nRF52840 = 0xADA52840，nRF52832 = 0x72718D47）。

use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::events::EventBus;

pub const BLOCK_SIZE: usize = 512;
pub const MAGIC_START0: u32 = 0x0A32_4655;
pub const MAGIC_START1: u32 = 0x9E5D_5157;
pub const MAGIC_END: u32 = 0x0AB1_6F30;
pub const FLAG_FAMILY_ID: u32 = 0x0000_2000;
pub const MAX_PAYLOAD: usize = 476;

/// UF2 镜像校验摘要。
#[derive(Debug, Clone, Serialize)]
pub struct Uf2Summary {
    /// familyID（十六进制字符串，如 "0xADA52840"）；镜像未携带时为 None。
    pub family_id: Option<String>,
    pub num_blocks: u32,
    pub payload_bytes: u64,
    pub addr_min: u32,
    pub addr_max: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum Uf2Error {
    #[error("file size {0} is not a multiple of 512")]
    BadSize(usize),
    #[error("block {index} has invalid magic")]
    BadMagic { index: usize },
    #[error("block {index} payload size {size} out of bounds")]
    BadPayload { index: usize, size: u32 },
    #[error("block sequence broken: expected {expected}, got {actual}")]
    BadSequence { expected: u32, actual: u32 },
    #[error("zero blocks")]
    Empty,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn read_u32_le(block: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(block[offset..offset + 4].try_into().expect("in-bounds u32"))
}

/// 校验 UF2 镜像并返回摘要。
pub fn validate(bytes: &[u8]) -> Result<Uf2Summary, Uf2Error> {
    if bytes.is_empty() {
        return Err(Uf2Error::Empty);
    }
    if !bytes.len().is_multiple_of(BLOCK_SIZE) {
        return Err(Uf2Error::BadSize(bytes.len()));
    }
    let num_blocks = (bytes.len() / BLOCK_SIZE) as u32;
    let mut family_id = None;
    let mut payload_bytes = 0u64;
    let mut addr_min = u32::MAX;
    let mut addr_max = 0u32;
    let mut declared_blocks = None;

    for (index, block) in bytes.as_chunks::<BLOCK_SIZE>().0.iter().enumerate() {
        let m0 = read_u32_le(block, 0);
        let m1 = read_u32_le(block, 4);
        let me = read_u32_le(block, BLOCK_SIZE - 4);
        if m0 != MAGIC_START0 || m1 != MAGIC_START1 || me != MAGIC_END {
            return Err(Uf2Error::BadMagic { index });
        }
        let flags = read_u32_le(block, 8);
        let target_addr = read_u32_le(block, 12);
        let payload_size = read_u32_le(block, 16);
        let block_no = read_u32_le(block, 20);
        let num = read_u32_le(block, 24);

        if payload_size as usize > MAX_PAYLOAD {
            return Err(Uf2Error::BadPayload {
                index,
                size: payload_size,
            });
        }
        if block_no as usize != index {
            return Err(Uf2Error::BadSequence {
                expected: index as u32,
                actual: block_no,
            });
        }
        match declared_blocks {
            None => declared_blocks = Some(num),
            Some(n) if n != num => {
                return Err(Uf2Error::BadSequence {
                    expected: n,
                    actual: num,
                })
            }
            _ => {}
        }
        if flags & FLAG_FAMILY_ID != 0 {
            family_id.get_or_insert(format!("0x{:08X}", read_u32_le(block, 28)));
        }
        payload_bytes += u64::from(payload_size);
        addr_min = addr_min.min(target_addr);
        addr_max = addr_max.max(target_addr + payload_size);
    }

    Ok(Uf2Summary {
        family_id,
        num_blocks,
        payload_bytes,
        addr_min,
        addr_max,
    })
}

/// 一个检测到的 UF2 Bootloader 卷。
#[derive(Debug, Clone, Serialize)]
pub struct Uf2Volume {
    /// 挂载点（macOS: /Volumes/XXX，Windows: E:\）。
    pub mount: String,
    /// INFO_UF2.TXT 内容（含 Model / Board-ID）。
    pub info: String,
}

/// 枚举当前挂载的 UF2 Bootloader 卷。
///
/// 判定条件：卷根存在 `INFO_UF2.TXT`。不过滤可移动标志——部分
/// bootloader 在 macOS 上不报 removable。
pub fn find_volumes() -> Vec<Uf2Volume> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut volumes = Vec::new();
    for disk in disks.list() {
        let mount = disk.mount_point();
        let info_path = mount.join("INFO_UF2.TXT");
        if let Ok(info) = fs::read_to_string(&info_path) {
            volumes.push(Uf2Volume {
                mount: mount.to_string_lossy().to_string(),
                info,
            });
        }
    }
    volumes
}

/// 轮询等待 UF2 卷出现（设备进入 Bootloader 后由插件调用）。
pub fn wait_for_volume(
    timeout: std::time::Duration,
    poll: std::time::Duration,
) -> Option<Uf2Volume> {
    let start = std::time::Instant::now();
    loop {
        let volumes = find_volumes();
        if let Some(v) = volumes.into_iter().next() {
            return Some(v);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(poll);
    }
}

/// 把 UF2 镜像写入指定卷，按块发布 `uf2.progress` 事件，完成后回读校验大小。
///
/// `job` 为进度事件关联 id。进度 payload：
/// `{job, written, total, percent}`。
pub fn flash(
    bus: &EventBus,
    job: &str,
    image_path: &Path,
    volume_mount: &str,
) -> Result<Uf2Summary, Uf2Error> {
    let bytes = fs::read(image_path)?;
    let summary = validate(&bytes)?;

    // 目标文件名沿用源文件名；bootloader 只关心块内容。
    let file_name = image_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "firmware.uf2".to_string());
    let target = PathBuf::from(volume_mount).join(file_name);

    let mut out = fs::File::create(&target)?;
    let total = bytes.len();
    let mut written = 0usize;
    for (index, block) in bytes.as_chunks::<BLOCK_SIZE>().0.iter().enumerate() {
        out.write_all(block)?;
        written += block.len();
        // 每 16 块（8KB）发一次进度，避免事件洪泛。
        if index % 16 == 0 || written == total {
            bus.publish(
                "uf2.progress",
                serde_json::json!({
                    "job": job,
                    "written": written,
                    "total": total,
                    "percent": (written * 100 / total),
                }),
            );
        }
    }
    out.flush()?;
    // 触发按块落盘。
    let _ = out.sync_all();

    // 回读校验：bootloader 常在接收完成后自动卸载卷，读不到不算失败。
    match fs::metadata(&target) {
        Ok(meta) if meta.len() as usize != total => {
            return Err(Uf2Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("write-back size mismatch: {} != {total}", meta.len()),
            )));
        }
        _ => {}
    }

    bus.publish(
        "uf2.progress",
        serde_json::json!({ "job": job, "written": total, "total": total, "percent": 100, "done": true }),
    );
    Ok(summary)
}

/// 生成一个内存中的合法 UF2 镜像（测试用）。
#[cfg(test)]
pub fn build_test_image(num_blocks: u32, payload: usize, family: u32) -> Vec<u8> {
    let mut image = Vec::new();
    for i in 0..num_blocks {
        let mut block = [0u8; BLOCK_SIZE];
        let put = |block: &mut [u8], off: usize, v: u32| {
            block[off..off + 4].copy_from_slice(&v.to_le_bytes())
        };
        put(&mut block, 0, MAGIC_START0);
        put(&mut block, 4, MAGIC_START1);
        put(&mut block, 8, FLAG_FAMILY_ID);
        put(&mut block, 12, 0x1000 + i * payload as u32);
        put(&mut block, 16, payload as u32);
        put(&mut block, 20, i);
        put(&mut block, 24, num_blocks);
        put(&mut block, 28, family);
        put(&mut block, BLOCK_SIZE - 4, MAGIC_END);
        image.extend_from_slice(&block);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_ok() {
        let image = build_test_image(4, 256, 0xADA5_2840);
        let s = validate(&image).unwrap();
        assert_eq!(s.num_blocks, 4);
        assert_eq!(s.payload_bytes, 1024);
        assert_eq!(s.family_id.as_deref(), Some("0xADA52840"));
        assert_eq!(s.addr_min, 0x1000);
        assert_eq!(s.addr_max, 0x1000 + 3 * 256 + 256);
    }

    #[test]
    fn validate_rejects_bad_size() {
        assert!(matches!(validate(&[0u8; 100]), Err(Uf2Error::BadSize(100))));
        assert!(matches!(validate(&[]), Err(Uf2Error::Empty)));
    }

    #[test]
    fn validate_rejects_bad_magic() {
        let mut image = build_test_image(2, 64, 0xADA5_2840);
        image[512] = 0xFF;
        assert!(matches!(
            validate(&image),
            Err(Uf2Error::BadMagic { index: 1 })
        ));
    }

    #[test]
    fn validate_rejects_sequence_gap() {
        let mut image = build_test_image(2, 64, 0xADA5_2840);
        // 第二块的 blockNo（偏移 512+20）改为 5
        image[512 + 20..512 + 24].copy_from_slice(&5u32.to_le_bytes());
        assert!(matches!(
            validate(&image),
            Err(Uf2Error::BadSequence {
                expected: 1,
                actual: 5
            })
        ));
    }
}
