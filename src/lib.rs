// SPDX-License-Identifier: Apache-2.0

//! Library interface for parsing Kubernetes condition JSON and formatting tables.
//!
//! Developed with assistance from the [Cursor](https://cursor.com) AI coding agent; human
//! maintainers retain responsibility for the result.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct ConditionRow {
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub last_transition: String,
    pub last_transition_ts: Option<DateTime<Utc>>,
}

impl ConditionRow {
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.type_.clone(),
            self.status.clone(),
            self.reason.clone(),
            self.last_transition.clone(),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    Type,
    Status,
    Time,
    Unsorted,
}

/// For time sort: larger (more recent) first; missing timestamps last.
pub fn cmp_last_transition_time(a: &ConditionRow, b: &ConditionRow) -> Ordering {
    match (&a.last_transition_ts, &b.last_transition_ts) {
        (Some(ta), Some(tb)) => tb.cmp(ta),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub fn sort_rows(rows: &mut [ConditionRow], mode: SortMode, reverse: bool) {
    let apply = |ord: Ordering| if reverse { ord.reverse() } else { ord };
    match mode {
        SortMode::Unsorted => {}
        SortMode::Type => rows.sort_by(|a, b| {
            apply(
                a.type_
                    .cmp(&b.type_)
                    .then_with(|| a.status.cmp(&b.status))
                    .then_with(|| a.reason.cmp(&b.reason)),
            )
        }),
        SortMode::Status => rows.sort_by(|a, b| {
            apply(
                a.status
                    .cmp(&b.status)
                    .then_with(|| a.type_.cmp(&b.type_))
                    .then_with(|| a.reason.cmp(&b.reason)),
            )
        }),
        SortMode::Time => rows.sort_by(|a, b| {
            apply(
                cmp_last_transition_time(a, b).then_with(|| a.type_.cmp(&b.type_)),
            )
        }),
    }
}

pub fn walk_path<'a>(mut v: &'a Value, path: &str) -> Result<Vec<&'a Value>> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Err(anyhow!("path must contain at least one segment"));
    }
    for seg in segments {
        v = v
            .get(seg)
            .ok_or_else(|| anyhow!("missing key {:?} in path {:?}", seg, path))?;
    }
    match v {
        Value::Array(arr) => Ok(arr.iter().collect()),
        Value::Object(_) => Ok(vec![v]),
        _ => Err(anyhow!(
            "value at path {:?} is not an array or object (got {})",
            path,
            json_kind(v)
        )),
    }
}

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn str_field(obj: &serde_json::Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "-".to_string())
}

pub fn parse_last_transition_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Human-readable age string for a RFC3339 `lastTransitionTime`, relative to `now`.
pub fn relative_time_at(raw: &str, now: DateTime<Utc>) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(raw) else {
        return raw.to_string();
    };
    let dt_utc = dt.with_timezone(&Utc);
    let dur = now.signed_duration_since(dt_utc);
    if dur.num_seconds() < 0 {
        return format!("in {}", format_duration(-dur));
    }
    format!("{} ago", format_duration(dur))
}

pub fn format_duration(d: chrono::Duration) -> String {
    let secs = d.num_seconds().abs();
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 48 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    format!("{days}d")
}

pub fn condition_row_from_object(
    obj: &serde_json::Map<String, Value>,
    now: DateTime<Utc>,
) -> ConditionRow {
    let raw_ltt = obj.get("lastTransitionTime").and_then(|v| v.as_str());
    let last_transition = raw_ltt
        .map(|s| relative_time_at(s, now))
        .unwrap_or_else(|| "-".to_string());
    let last_transition_ts = raw_ltt.and_then(parse_last_transition_time);
    ConditionRow {
        type_: str_field(obj, "type"),
        status: str_field(obj, "status"),
        reason: str_field(obj, "reason"),
        last_transition,
        last_transition_ts,
    }
}

const COL_SEP: &str = "  ";

fn cell_width(s: &str) -> usize {
    s.chars().count()
}

pub fn column_widths(header: Option<&[String]>, rows: &[Vec<String>]) -> Vec<usize> {
    const NCOLS: usize = 4;
    let mut widths = vec![0usize; NCOLS];
    if let Some(h) = header {
        for (i, cell) in h.iter().enumerate().take(NCOLS) {
            widths[i] = widths[i].max(cell_width(cell));
        }
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(NCOLS) {
            widths[i] = widths[i].max(cell_width(cell));
        }
    }
    widths
}

/// One formatted table line (no trailing newline).
/// Padding uses Unicode scalar count, matching [`cell_width`].
pub fn format_padded_line(cols: &[String], widths: &[usize]) -> String {
    let mut out = String::new();
    for (i, cell) in cols.iter().enumerate() {
        if i > 0 {
            out.push_str(COL_SEP);
        }
        let target = widths.get(i).copied().unwrap_or(0);
        out.push_str(cell);
        let pad = target.saturating_sub(cell.chars().count());
        out.extend(std::iter::repeat(' ').take(pad));
    }
    out
}

pub const TABLE_HEADER: &[&str] = &["TYPE", "STATUS", "REASON", "LAST_TRANSITION"];

pub fn table_header_strings() -> Vec<String> {
    TABLE_HEADER.iter().map(|s| (*s).to_string()).collect()
}

/// `true` when `root` looks like a Kubernetes `List` (`kind: List` with an `items` array).
pub fn is_kubernetes_list(root: &Value) -> bool {
    let Some(obj) = root.as_object() else {
        return false;
    };
    if obj.get("kind").and_then(|v| v.as_str()) != Some("List") {
        return false;
    }
    obj.get("items").is_some_and(|v| v.is_array())
}

/// References to each entry in a Kubernetes list document, sorted by namespace then name.
pub fn kubernetes_list_items_sorted<'a>(root: &'a Value) -> Option<Vec<&'a Value>> {
    if !is_kubernetes_list(root) {
        return None;
    }
    let items = root.get("items")?.as_array()?;
    let mut refs: Vec<&Value> = items.iter().collect();
    refs.sort_by(|a, b| resource_sort_key(a).cmp(&resource_sort_key(b)));
    Some(refs)
}

/// Namespace and name for ordering (`namespace` is `""` when absent, e.g. cluster-scoped).
pub fn resource_sort_key(obj: &Value) -> (String, String) {
    let meta = obj.get("metadata").and_then(|m| m.as_object());
    let ns = meta
        .and_then(|m| m.get("namespace"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = meta
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (ns, name)
}

/// One-line label for a resource stanza (`Kind name` or `Kind namespace/name`).
pub fn resource_stanza_title(obj: &Value) -> String {
    let kind = obj.get("kind").and_then(|v| v.as_str()).unwrap_or("Object");
    let (ns, name) = resource_sort_key(obj);
    if name.is_empty() {
        kind.to_string()
    } else if ns.is_empty() {
        format!("{kind} {name}")
    } else {
        format!("{kind} {ns}/{name}")
    }
}

/// Format stdin-style JSON: either one object or a Kubernetes `List` from `kubectl get … -o json`.
///
/// For a **List**, items are ordered by namespace then name. Each item gets a title line, then
/// the condition table (`path` is evaluated relative to that item). Condition sorting uses
/// `mode` / `reverse` per item. For a **single object**, behavior matches [`format_condition_table`]
/// (no title line).
pub fn format_kubernetes_document(
    root: &Value,
    path: &str,
    mode: SortMode,
    reverse: bool,
    no_header: bool,
    now: DateTime<Utc>,
) -> Result<Vec<String>> {
    if let Some(items) = kubernetes_list_items_sorted(root) {
        let mut all = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                all.push(String::new());
            }
            let title = resource_stanza_title(item);
            all.push(title);
            match format_condition_table(item, path, mode, reverse, no_header, now) {
                Ok(mut lines) => all.append(&mut lines),
                Err(e) => all.push(format!("  # {e}")),
            }
        }
        Ok(all)
    } else {
        format_condition_table(root, path, mode, reverse, no_header, now)
    }
}

/// Full table as lines (including optional header), for tests and inspection.
pub fn format_condition_table(
    root: &Value,
    path: &str,
    mode: SortMode,
    reverse: bool,
    no_header: bool,
    now: DateTime<Utc>,
) -> Result<Vec<String>> {
    let items = walk_path(root, path)?;
    let mut rows: Vec<ConditionRow> = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        rows.push(condition_row_from_object(obj, now));
    }
    sort_rows(&mut rows, mode, reverse);
    let display_rows: Vec<Vec<String>> = rows.iter().map(ConditionRow::to_cells).collect();
    let header = table_header_strings();
    let widths = column_widths(
        if no_header {
            None
        } else {
            Some(header.as_slice())
        },
        &display_rows,
    );
    let mut lines = Vec::new();
    if !no_header {
        lines.push(format_padded_line(&header, &widths));
    }
    for row in &display_rows {
        lines.push(format_padded_line(row, &widths));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-02T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn walk_path_empty_segment_error() {
        let v = serde_json::json!({});
        let err = walk_path(&v, "").unwrap_err();
        assert!(err.to_string().contains("at least one segment"));
    }

    #[test]
    fn walk_path_missing_key() {
        let v = serde_json::json!({"status": {}});
        let err = walk_path(&v, "status.conditions").unwrap_err();
        assert!(err.to_string().contains("missing key"));
    }

    #[test]
    fn walk_path_leaf_not_array_or_object() {
        let v = serde_json::json!({"status": {"conditions": "nope"}});
        let err = walk_path(&v, "status.conditions").unwrap_err();
        assert!(err.to_string().contains("not an array or object"));
    }

    #[test]
    fn walk_path_array_ok() {
        let v = serde_json::json!({"a": {"b": [1, 2]}});
        let got = walk_path(&v, "a.b").unwrap();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn walk_path_single_object_ok() {
        let v = serde_json::json!({"x": {"y": {"type": "T"}}});
        let got = walk_path(&v, "x.y").unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn invalid_json_from_str() {
        let res = serde_json::from_str::<Value>("{not json");
        assert!(res.is_err());
    }

    #[test]
    fn relative_time_at_minutes_ago() {
        let now = fixed_now();
        assert_eq!(
            relative_time_at("2026-04-02T14:47:00Z", now),
            "13m ago"
        );
    }

    #[test]
    fn relative_time_at_unparseable_passthrough() {
        assert_eq!(
            relative_time_at("not-a-date", fixed_now()),
            "not-a-date"
        );
    }

    #[test]
    fn format_duration_units() {
        assert_eq!(format_duration(chrono::Duration::seconds(30)), "30s");
        assert_eq!(format_duration(chrono::Duration::seconds(120)), "2m");
        assert_eq!(format_duration(chrono::Duration::seconds(7200)), "2h");
        assert_eq!(format_duration(chrono::Duration::seconds(172800)), "2d");
    }

    #[test]
    fn column_widths_and_padding_align() {
        let header = vec![
            "TYPE".into(),
            "STATUS".into(),
            "REASON".into(),
            "LAST_TRANSITION".into(),
        ];
        let rows: Vec<Vec<String>> = vec![vec![
            "Short".into(),
            "True".into(),
            "-".into(),
            "1m ago".into(),
        ]];
        let w = column_widths(Some(&header), &rows);
        assert_eq!(w, vec![5, 6, 6, 15]);
        let line = format_padded_line(&rows[0], &w);
        // After the STATUS column ("True  ") comes COL_SEP ("  "), so there are four spaces after "e".
        assert_eq!(
            line,
            "Short  True    -       1m ago         "
        );
        let hdr = format_padded_line(&header, &w);
        assert!(hdr.starts_with("TYPE"));
        assert!(hdr.contains("LAST_TRANSITION"));
    }

    #[test]
    fn sort_by_type_default_order() {
        let now = fixed_now();
        let mut rows = vec![
            ConditionRow {
                type_: "Zebra".into(),
                status: "True".into(),
                reason: "-".into(),
                last_transition: relative_time_at("2026-04-01T00:00:00Z", now),
                last_transition_ts: parse_last_transition_time("2026-04-01T00:00:00Z"),
            },
            ConditionRow {
                type_: "Alpha".into(),
                status: "False".into(),
                reason: "-".into(),
                last_transition: relative_time_at("2026-04-02T00:00:00Z", now),
                last_transition_ts: parse_last_transition_time("2026-04-02T00:00:00Z"),
            },
        ];
        sort_rows(&mut rows, SortMode::Type, false);
        assert_eq!(rows[0].type_, "Alpha");
        assert_eq!(rows[1].type_, "Zebra");
    }

    #[test]
    fn sort_by_time_most_recent_first() {
        let mut rows = vec![
            ConditionRow {
                type_: "A".into(),
                status: "True".into(),
                reason: "-".into(),
                last_transition: String::new(),
                last_transition_ts: parse_last_transition_time("2026-04-01T00:00:00Z"),
            },
            ConditionRow {
                type_: "B".into(),
                status: "True".into(),
                reason: "-".into(),
                last_transition: String::new(),
                last_transition_ts: parse_last_transition_time("2026-04-02T00:00:00Z"),
            },
        ];
        sort_rows(&mut rows, SortMode::Time, false);
        assert_eq!(rows[0].type_, "B");
        assert_eq!(rows[1].type_, "A");
    }

    /// Anonymized fixture inspired by pod-style conditions (types/reasons are generic).
    fn sample_cr_json() -> Value {
        serde_json::json!({
            "status": {
                "conditions": [
                    {"type": "CondTypeLongNameOne", "status": "True", "reason": "-", "lastTransitionTime": "2026-04-02T14:47:00Z"},
                    {"type": "CondTypeB", "status": "False", "reason": "ReasonAlpha", "lastTransitionTime": "2026-04-02T14:47:00Z"},
                    {"type": "CondTypeC", "status": "False", "reason": "ReasonBeta", "lastTransitionTime": "2026-04-02T14:47:00Z"},
                    {"type": "CondTypeD", "status": "False", "reason": "ReasonBeta", "lastTransitionTime": "2026-04-02T14:47:00Z"},
                    {"type": "CondTypeE", "status": "True", "reason": "-", "lastTransitionTime": "2026-04-02T14:47:00Z"}
                ]
            }
        })
    }

    #[test]
    fn format_table_matches_expected_columns_and_type_sort() {
        let root = sample_cr_json();
        let now = fixed_now();
        let lines = format_condition_table(
            &root,
            "status.conditions",
            SortMode::Type,
            false,
            false,
            now,
        )
        .unwrap();
        assert_eq!(lines.len(), 6);
        assert!(lines[0].contains("TYPE"));
        assert!(lines[0].contains("LAST_TRANSITION"));
        // Sorted by type: CondTypeB, C, D, E, LongNameOne
        assert!(lines[1].starts_with("CondTypeB"));
        assert!(lines[2].starts_with("CondTypeC"));
        assert!(lines[5].starts_with("CondTypeLongNameOne"));
        for line in &lines[1..] {
            assert!(
                line.contains("13m ago"),
                "expected fixed relative time: {line}"
            );
        }
    }

    #[test]
    fn format_table_unsorted_preserves_json_order() {
        let root = sample_cr_json();
        let now = fixed_now();
        let lines = format_condition_table(
            &root,
            "status.conditions",
            SortMode::Unsorted,
            false,
            true,
            now,
        )
        .unwrap();
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with("CondTypeLongNameOne"));
        assert!(lines[1].starts_with("CondTypeB"));
    }

    #[test]
    fn single_document_no_stanza_title() {
        let root = sample_cr_json();
        let now = fixed_now();
        let lines = format_kubernetes_document(
            &root,
            "status.conditions",
            SortMode::Type,
            false,
            false,
            now,
        )
        .unwrap();
        assert!(
            lines[0].contains("TYPE"),
            "expected table header first, got {:?}",
            lines.first()
        );
    }

    #[test]
    fn kubernetes_list_sorts_resources_by_namespace_then_name() {
        let root = serde_json::json!({
            "kind": "List",
            "apiVersion": "v1",
            "items": [
                {"kind": "Pod", "metadata": {"name": "zebra", "namespace": "ns-b"}, "status": {"conditions": [
                    {"type": "Ready", "status": "True", "lastTransitionTime": "2026-04-02T14:47:00Z"}
                ]}},
                {"kind": "Pod", "metadata": {"name": "alpha", "namespace": "ns-a"}, "status": {"conditions": [
                    {"type": "Ready", "status": "True", "lastTransitionTime": "2026-04-02T14:47:00Z"}
                ]}},
                {"kind": "Pod", "metadata": {"name": "mule", "namespace": "ns-a"}, "status": {"conditions": [
                    {"type": "Ready", "status": "True", "lastTransitionTime": "2026-04-02T14:47:00Z"}
                ]}},
            ]
        });
        let now = fixed_now();
        let lines = format_kubernetes_document(
            &root,
            "status.conditions",
            SortMode::Type,
            false,
            true,
            now,
        )
        .unwrap();
        let joined = lines.join("\n");
        let pos_a_mule = joined.find("Pod ns-a/mule").expect("mule stanza");
        let pos_a_alpha = joined.find("Pod ns-a/alpha").expect("alpha stanza");
        let pos_b_zebra = joined.find("Pod ns-b/zebra").expect("zebra stanza");
        assert!(pos_a_alpha < pos_a_mule);
        assert!(pos_a_mule < pos_b_zebra);
    }

    #[test]
    fn kubernetes_list_sorts_conditions_within_each_item() {
        let root = serde_json::json!({
            "kind": "List",
            "items": [
                {"kind": "Pod", "metadata": {"name": "p", "namespace": "n"}, "status": {"conditions": [
                    {"type": "Zeta", "status": "True", "lastTransitionTime": "2026-04-02T14:47:00Z"},
                    {"type": "Alpha", "status": "True", "lastTransitionTime": "2026-04-02T14:47:00Z"},
                ]}},
            ]
        });
        let now = fixed_now();
        let lines = format_kubernetes_document(
            &root,
            "status.conditions",
            SortMode::Type,
            false,
            true,
            now,
        )
        .unwrap();
        let idx_alpha = lines
            .iter()
            .position(|l| l.starts_with("Alpha"))
            .expect("Alpha row");
        let idx_zeta = lines
            .iter()
            .position(|l| l.starts_with("Zeta"))
            .expect("Zeta row");
        assert!(idx_alpha < idx_zeta);
    }
}
