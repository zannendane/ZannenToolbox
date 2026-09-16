//! 设备指纹：依据 `devices.json` 识别 Zannen 硬件。
//!
//! 证据源（任一命中即识别）：
//! 1. 串口 identify 应答 `hello.hw` 或控制台 `info` 的 `Board:` 行 ↔ `hw_names`
//! 2. BLE 广播名子串 ↔ `ble_name_patterns` / 服务 UUID ↔ `services`
//! 3. USB VID/PID ↔ `usb`；USB product 字符串子串（最长匹配优先）↔ `usb_product_patterns`
//!    —— 应用态 PID 在 Smol/SmolAir、Dongle/Dongle33 间复用，product 是区分依据。

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct FingerprintDb {
    pub devices: Vec<Fingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub mcu: Option<String>,
    #[serde(default)]
    pub uf2_family: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(rename = "match")]
    pub criteria: MatchCriteria,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchCriteria {
    #[serde(default)]
    pub hw_names: Vec<String>,
    #[serde(default)]
    pub ble_name_patterns: Vec<String>,
    #[serde(default)]
    pub usb: Vec<UsbId>,
    #[serde(default)]
    pub usb_product_patterns: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbId {
    pub vid: String,
    pub pid: String,
}

impl FingerprintDb {
    pub fn parse(json_text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_text)
    }

    /// 按 hello 应答的 hw 字段匹配（大小写不敏感精确匹配）。
    pub fn by_hw_name(&self, hw: &str) -> Option<&Fingerprint> {
        let hw = hw.to_lowercase();
        self.devices
            .iter()
            .find(|f| f.criteria.hw_names.iter().any(|n| n.to_lowercase() == hw))
    }

    /// 按 BLE 广播名（子串，大小写不敏感）或服务 UUID 匹配。
    ///
    /// 名称模式可能互相包含（如 "zannensmol" 是 "zannensmolair" 的子串），
    /// 因此命中多个指纹时取**模式最长**者（最具体优先）。
    pub fn by_ble(&self, name: Option<&str>, services: &[String]) -> Option<&Fingerprint> {
        let name = name.map(str::to_lowercase);
        self.devices
            .iter()
            .filter_map(|f| {
                let name_score = name.as_ref().and_then(|n| {
                    f.criteria
                        .ble_name_patterns
                        .iter()
                        .filter(|p| n.contains(&p.to_lowercase()))
                        .map(|p| p.len())
                        .max()
                });
                let service_hit = f
                    .criteria
                    .services
                    .iter()
                    .any(|s| services.iter().any(|seen| seen.eq_ignore_ascii_case(s)));
                // 服务命中视为最高优先级（UUID 全局唯一）
                let score = if service_hit { usize::MAX } else { name_score? };
                Some((score, f))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, f)| f)
    }

    /// 按 USB VID/PID 匹配（十六进制字符串，大小写不敏感）。
    pub fn by_usb(&self, vid: &str, pid: &str) -> Option<&Fingerprint> {
        self.devices.iter().find(|f| {
            f.criteria
                .usb
                .iter()
                .any(|u| u.vid.eq_ignore_ascii_case(vid) && u.pid.eq_ignore_ascii_case(pid))
        })
    }

    /// 按 USB product 字符串匹配（子串，大小写不敏感，最长模式优先）。
    ///
    /// 应用态 VID/PID 在多型设备间复用（见 devices.json 注释），
    /// product 字符串（如 "SlimeVR ZannenSmolAir"）是区分型号的依据；
    /// 模式可能互相包含，故取**最长命中**者。
    pub fn by_usb_product(&self, product: &str) -> Option<&Fingerprint> {
        let product = product.to_lowercase();
        self.devices
            .iter()
            .filter_map(|f| {
                f.criteria
                    .usb_product_patterns
                    .iter()
                    .filter(|p| product.contains(&p.to_lowercase()))
                    .map(|p| p.len())
                    .max()
                    .map(|score| (score, f))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, f)| f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB: &str = include_str!("../devices.json");

    #[test]
    fn parses_embedded_db() {
        let db = FingerprintDb::parse(DB).unwrap();
        assert_eq!(db.devices.len(), 4);
        assert_eq!(db.devices[0].kind, "zannen-smol");
    }

    #[test]
    fn matches_hello() {
        let db = FingerprintDb::parse(DB).unwrap();
        assert_eq!(db.by_hw_name("Zannen-Smol").unwrap().kind, "zannen-smol");
        // 真实固件 info 命令的 Board: 行（Zephyr board target 名）
        assert_eq!(
            db.by_hw_name("zannensmolair_uf2").unwrap().kind,
            "zannen-smol-air"
        );
        assert_eq!(
            db.by_hw_name("zannen-dongle").unwrap().mcu.as_deref(),
            Some("nRF52840")
        );
        assert!(db.by_hw_name("unknown-board").is_none());
    }

    #[test]
    fn matches_ble_name() {
        let db = FingerprintDb::parse(DB).unwrap();
        let f = db.by_ble(Some("ZannenSmolAir-AB12"), &[]).unwrap();
        assert_eq!(f.kind, "zannen-smol-air");
        assert!(f.capabilities.contains(&"ble".to_string()));
        // 应用固件无 BLE 服务 UUID 指纹（虚构 NUS 已移除）
        assert!(db
            .by_ble(None, &["6E400001-B5A3-F393-E0A9-E50E24DCCA9E".to_string()])
            .is_none());
        assert!(db.by_ble(Some("SomeRandomBle"), &[]).is_none());
    }

    #[test]
    fn matches_usb() {
        let db = FingerprintDb::parse(DB).unwrap();
        let f = db.by_usb("0x1209", "0x7690").unwrap();
        assert_eq!(f.kind, "zannen-dongle");
        assert!(db.by_usb("0x0000", "0x0000").is_none());
    }

    #[test]
    fn matches_usb_product_longest_wins() {
        let db = FingerprintDb::parse(DB).unwrap();
        // VID/PID 复用场景靠 product 字符串区分，且长模式优先
        assert_eq!(
            db.by_usb_product("SlimeVR ZannenSmol").unwrap().kind,
            "zannen-smol"
        );
        assert_eq!(
            db.by_usb_product("SlimeVR ZannenSmolAir").unwrap().kind,
            "zannen-smol-air"
        );
        assert_eq!(
            db.by_usb_product("SlimeNRF Receiver ZannenDongle33")
                .unwrap()
                .kind,
            "zannen-dongle33"
        );
        // bootloader 态 product
        assert_eq!(
            db.by_usb_product("ZannenDongle_nRF52840").unwrap().kind,
            "zannen-dongle"
        );
        assert!(db.by_usb_product("Some Random Gadget").is_none());
    }
}
