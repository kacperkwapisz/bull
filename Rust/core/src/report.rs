use std::{fs, path::Path};

use serde::Serialize;

use crate::{BullError, BullResult};

pub fn write_json_report<T: Serialize>(report: &T, output: Option<&Path>) -> BullResult<()> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|source| BullError::message(format!("cannot serialize report: {source}")))?;

    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| BullError::io(parent, source))?;
        }
        fs::write(path, json.as_bytes()).map_err(|source| BullError::io(path, source))?;
    }

    println!("{json}");
    Ok(())
}
