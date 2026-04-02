// SPDX-License-Identifier: Apache-2.0
// Developed with assistance from the Cursor AI coding agent (https://cursor.com).

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use prettycond::{format_kubernetes_document, SortMode};
use serde_json::Value;
use std::io::{self, Read};

#[derive(Parser, Debug)]
#[command(name = "prettycond")]
#[command(
    about = "Read a Kubernetes CR or List JSON from STDIN and print status conditions as columns."
)]
struct Args {
    /// Dot-separated JSON path to the conditions array (e.g. status.conditions)
    #[arg(long, default_value = "status.conditions")]
    path: String,

    /// Skip the header row
    #[arg(long)]
    no_header: bool,

    /// Reverse sort order
    #[arg(short = 'r', long)]
    reverse: bool,

    /// Do not sort; preserve order from the JSON array (like ls -U)
    #[arg(short = 'U', long = "unsorted", group = "sort_key")]
    unsorted: bool,

    /// Sort by status column
    #[arg(short = 's', long = "sort-status", group = "sort_key")]
    sort_status: bool,

    /// Sort by last transition time, most recent first
    #[arg(short = 't', long = "sort-time", group = "sort_key")]
    sort_time: bool,
}

fn sort_mode_from_args(args: &Args) -> SortMode {
    if args.unsorted {
        SortMode::Unsorted
    } else if args.sort_status {
        SortMode::Status
    } else if args.sort_time {
        SortMode::Time
    } else {
        SortMode::Type
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("read STDIN")?;

    let root: Value = serde_json::from_str(&buf).context("parse JSON from STDIN")?;

    let now = Utc::now();
    let mode = sort_mode_from_args(&args);
    let lines = format_kubernetes_document(
        &root,
        &args.path,
        mode,
        args.reverse,
        args.no_header,
        now,
    )?;

    for line in lines {
        println!("{line}");
    }

    Ok(())
}
