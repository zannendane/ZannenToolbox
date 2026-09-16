//! 状态解读规则引擎：把设备 `status` 数据包映射为人类可读卡片。
//!
//! 规则表见 `status_rules.json`。`when` 表达式支持：
//! `<= N` / `>= N` / `< N` / `> N`（数值比较）、`== X`（数值/字符串/布尔相等）、
//! `default`（兜底）。levels 按声明顺序求值，首个命中生效。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct RulesDoc {
    pub cards: Vec<CardRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardRule {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    /// status 包内字段名。
    pub source: String,
    #[serde(default)]
    pub unit: String,
    pub levels: Vec<LevelRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelRule {
    pub when: String,
    pub level: String,
    #[serde(default)]
    pub note: String,
}

impl RulesDoc {
    pub fn parse(json_text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_text)
    }
}

/// 对一条 status 消息执行全部卡片规则，输出 UI 卡片数组。
pub fn interpret(doc: &RulesDoc, status: &Value) -> Vec<Value> {
    doc.cards
        .iter()
        .filter_map(|card| {
            let value = status.get(&card.source).cloned()?;
            let hit = card.levels.iter().find(|l| eval_when(&l.when, &value));
            let (level, note) = hit
                .map(|l| (l.level.clone(), l.note.clone()))
                .unwrap_or_else(|| ("ok".into(), String::new()));
            Some(json!({
                "id": card.id,
                "title": card.title,
                "icon": card.icon,
                "value": value,
                "unit": card.unit,
                "level": level,
                "note": note,
            }))
        })
        .collect()
}

fn eval_when(when: &str, value: &Value) -> bool {
    let when = when.trim();
    if when == "default" {
        return true;
    }
    if let Some(rest) = when.strip_prefix("<=") {
        return cmp_num(value, rest, |a, b| a <= b);
    }
    if let Some(rest) = when.strip_prefix(">=") {
        return cmp_num(value, rest, |a, b| a >= b);
    }
    if let Some(rest) = when.strip_prefix('<') {
        return cmp_num(value, rest, |a, b| a < b);
    }
    if let Some(rest) = when.strip_prefix('>') {
        return cmp_num(value, rest, |a, b| a > b);
    }
    if let Some(rest) = when.strip_prefix("==") {
        let rest = rest.trim();
        // 布尔
        if let Ok(b) = rest.parse::<bool>() {
            return value.as_bool() == Some(b);
        }
        // 数值
        if let Ok(n) = rest.parse::<f64>() {
            return value.as_f64() == Some(n);
        }
        // 字符串
        return value.as_str() == Some(rest);
    }
    false
}

fn cmp_num(value: &Value, rhs: &str, f: impl Fn(f64, f64) -> bool) -> bool {
    match (value.as_f64(), rhs.trim().parse::<f64>()) {
        (Some(a), Ok(b)) => f(a, b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = include_str!("../status_rules.json");

    #[test]
    fn parses_embedded_rules() {
        let doc = RulesDoc::parse(RULES).unwrap();
        assert!(doc.cards.len() >= 4);
    }

    #[test]
    fn interprets_levels() {
        let doc = RulesDoc::parse(RULES).unwrap();
        let cards = interpret(
            &doc,
            &json!({"battery": 8.0, "charging": false, "imu": "ok", "rf_link": "weak", "fw": "1.4.2"}),
        );
        let by_id = |id: &str| cards.iter().find(|c| c["id"] == id).cloned().unwrap();
        assert_eq!(by_id("battery")["level"], "critical");
        assert_eq!(by_id("rf_link")["level"], "warn");
        assert_eq!(by_id("imu")["level"], "ok");
        assert_eq!(by_id("fw")["value"], "1.4.2");
        assert_eq!(
            by_id("charging")["note"],
            doc.cards
                .iter()
                .find(|c| c.id == "charging")
                .unwrap()
                .levels[1]
                .note
        );
    }

    #[test]
    fn missing_source_skips_card() {
        let doc = RulesDoc::parse(RULES).unwrap();
        let cards = interpret(&doc, &json!({"battery": 50.0}));
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0]["id"], "battery");
    }
}
