use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use serde_json::Value;
use std::io::{self, Read};

#[derive(Parser, Debug)]
#[command(name = "prettycond")]
#[command(about = "Read a Kubernetes CR from STDIN and print conditions as columns.")]
struct Args {
    /// Dot-separated JSON path to the conditions array (e.g. status.conditions)
    #[arg(long, default_value = "status.conditions")]
    path: String,

    /// Skip the header row
    #[arg(long)]
    no_header: bool,
}

fn walk_path<'a>(mut v: &'a Value, path: &str) -> Result<Vec<&'a Value>> {
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

fn relative_time(s: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(s) else {
        return s.to_string();
    };
    let dt_utc = dt.with_timezone(&Utc);
    let now = Utc::now();
    let dur = now.signed_duration_since(dt_utc);
    if dur.num_seconds() < 0 {
        return format!("in {}", format_duration(-dur));
    }
    format!("{} ago", format_duration(dur))
}

fn format_duration(d: chrono::Duration) -> String {
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

const COL_SEP: &str = "  ";

fn cell_width(s: &str) -> usize {
    s.chars().count()
}

fn column_widths(header: Option<&[String]>, rows: &[Vec<String>]) -> Vec<usize> {
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

fn print_padded_row(cols: &[String], widths: &[usize]) {
    let mut out = String::new();
    for (i, cell) in cols.iter().enumerate() {
        if i > 0 {
            out.push_str(COL_SEP);
        }
        let w = widths.get(i).copied().unwrap_or(0);
        out.push_str(&format!("{:<width$}", cell, width = w));
    }
    println!("{out}");
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("read STDIN")?;

    let root: Value = serde_json::from_str(&buf).context("parse JSON from STDIN")?;
    let items = walk_path(&root, &args.path)?;

    let header = [
        "TYPE".into(),
        "STATUS".into(),
        "REASON".into(),
        "LAST_TRANSITION".into(),
    ];

    let mut rows: Vec<Vec<String>> = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            eprintln!("warning: skipping non-object condition entry");
            continue;
        };
        let ltt = obj
            .get("lastTransitionTime")
            .and_then(|v| v.as_str())
            .map(relative_time)
            .unwrap_or_else(|| "-".to_string());

        rows.push(vec![
            str_field(obj, "type"),
            str_field(obj, "status"),
            str_field(obj, "reason"),
            ltt,
        ]);
    }

    let widths = column_widths(
        if args.no_header { None } else { Some(&header) },
        &rows,
    );

    if !args.no_header {
        print_padded_row(&header, &widths);
    }
    for row in &rows {
        print_padded_row(row, &widths);
    }

    Ok(())
}
