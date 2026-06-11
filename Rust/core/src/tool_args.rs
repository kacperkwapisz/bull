use std::{env, path::PathBuf};

use crate::{BullError, BullResult};

pub fn args() -> Vec<String> {
    env::args().skip(1).collect()
}

pub fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

pub fn value(args: &[String], name: &str) -> BullResult<Option<String>> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            let Some(value) = iter.next() else {
                return Err(BullError::message(format!("missing value for {name}")));
            };
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

pub fn path_value(args: &[String], name: &str) -> BullResult<Option<PathBuf>> {
    Ok(value(args, name)?.map(PathBuf::from))
}

pub fn default_path(args: &[String], name: &str, default: &str) -> BullResult<PathBuf> {
    Ok(path_value(args, name)?.unwrap_or_else(|| PathBuf::from(default)))
}
