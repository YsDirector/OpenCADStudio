//! Naming and collision protection for drawing-training deliverables.
//!
//! The training contract is deliberately small and deterministic:
//! `YSD06-<yymmdd>-<letter>.dxf` plus a same-stem evidence directory.  This
//! module does not decide whether a drawing is correct; it only prevents a
//! verified save from landing on a temporary name or silently replacing an
//! earlier deliverable.

use std::path::{Path, PathBuf};

pub const DELIVERY_PREFIX: &str = "YSD06";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryTarget {
    pub root: PathBuf,
    pub stem: String,
    pub date: String,
    pub letter: char,
    pub drawing: PathBuf,
    pub evidence_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryNameError {
    pub code: &'static str,
    pub message: String,
}

/// A short-lived create-new marker closes the scan/check/save race between
/// two automation clients.  Dropping it releases the name after a successful
/// or failed save; the drawing/evidence directory remain the durable claim.
pub struct DeliveryReservation {
    marker: PathBuf,
}

impl Drop for DeliveryReservation {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.marker);
    }
}

impl DeliveryNameError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DeliveryNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DeliveryNameError {}

fn valid_date(date: &str) -> bool {
    date.len() == 6 && date.as_bytes().iter().all(u8::is_ascii_digit)
}

fn parse_stem(stem: &str) -> Option<(&str, char)> {
    let mut parts = stem.split('-');
    if parts.next()? != DELIVERY_PREFIX {
        return None;
    }
    let date = parts.next()?;
    let suffix = parts.next()?;
    if parts.next().is_some() || !valid_date(date) {
        return None;
    }
    let mut chars = suffix.chars();
    let letter = chars.next()?;
    if chars.next().is_some() || !letter.is_ascii_lowercase() {
        return None;
    }
    Some((date, letter))
}

fn stem_from_entry(name: &str) -> Option<&str> {
    if parse_stem(name).is_some() {
        return Some(name);
    }
    if let Some(stem) = name
        .strip_prefix('.')
        .and_then(|value| value.strip_suffix(".delivery.lock"))
    {
        if parse_stem(stem).is_some() {
            return Some(stem);
        }
    }
    for extension in [".dxf", ".dwg", ".bak"] {
        if let Some(stem) = name.strip_suffix(extension) {
            if parse_stem(stem).is_some() {
                return Some(stem);
            }
        }
    }
    None
}

fn candidate(root: &Path, date: &str, letter: char) -> DeliveryTarget {
    let stem = format!("{DELIVERY_PREFIX}-{date}-{letter}");
    DeliveryTarget {
        root: root.to_path_buf(),
        stem: stem.clone(),
        date: date.to_string(),
        letter,
        drawing: root.join(format!("{stem}.dxf")),
        evidence_dir: root.join(stem),
    }
}

/// Find the first unused letter for a date.  Existing `.dxf`, `.dwg`, `.bak`
/// and same-stem evidence directories all reserve the letter.
pub fn next_target(root: &Path, date: &str) -> Result<DeliveryTarget, DeliveryNameError> {
    if !valid_date(date) {
        return Err(DeliveryNameError::new(
            "invalid_delivery_date",
            format!("delivery date must be yymmdd, got `{date}`"),
        ));
    }
    if !root.is_dir() {
        return Err(DeliveryNameError::new(
            "delivery_root_missing",
            format!("delivery root directory does not exist: {}", root.display()),
        ));
    }
    let mut used = [false; 26];
    let entries = std::fs::read_dir(root).map_err(|error| {
        DeliveryNameError::new(
            "delivery_root_unreadable",
            format!("cannot scan delivery root {}: {error}", root.display()),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            DeliveryNameError::new("delivery_root_unreadable", error.to_string())
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(stem) = stem_from_entry(name) else {
            continue;
        };
        if let Some((entry_date, letter)) = parse_stem(stem) {
            if entry_date == date {
                used[(letter as u8 - b'a') as usize] = true;
            }
        }
    }
    for index in 0..26 {
        if !used[index] {
            return Ok(candidate(root, date, (b'a' + index as u8) as char));
        }
    }
    Err(DeliveryNameError::new(
        "delivery_sequence_exhausted",
        format!("all YSD06-{date}-a..z names are already reserved"),
    ))
}

/// Use the local calendar date without adding a time/date dependency.
pub fn today_yymmdd() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days, with the Unix epoch offset.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    format!("{:02}{:02}{:02}", (y % 100 + 100) % 100, m, d)
}

/// Validate an explicitly selected delivery path.  The caller may create the
/// evidence directory after this check, but the drawing itself must be a new
/// `.dxf` in the requested root.
pub fn validate_target(root: &Path, path: &Path) -> Result<DeliveryTarget, DeliveryNameError> {
    if path.parent() != Some(root) {
        return Err(DeliveryNameError::new(
            "delivery_path_mismatch",
            format!(
                "delivery drawing must be directly inside {}",
                root.display()
            ),
        ));
    }
    if path.extension().and_then(|value| value.to_str()) != Some("dxf") {
        return Err(DeliveryNameError::new(
            "delivery_format_required",
            "training deliverables must use the .dxf extension",
        ));
    }
    let file_stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            DeliveryNameError::new(
                "delivery_name_invalid",
                "delivery path has no valid file name",
            )
        })?;
    let (date, letter) = parse_stem(file_stem).ok_or_else(|| {
        DeliveryNameError::new(
            "delivery_name_invalid",
            format!(
                "expected YSD06-<yymmdd>-<letter>.dxf, got `{}`",
                path.display()
            ),
        )
    })?;
    let target = DeliveryTarget {
        root: root.to_path_buf(),
        stem: file_stem.to_string(),
        date: date.to_string(),
        letter,
        drawing: path.to_path_buf(),
        evidence_dir: root.join(file_stem),
    };
    if path.exists() || target.evidence_dir.exists() || reservation_marker(&target).exists() {
        return Err(DeliveryNameError::new(
            "delivery_destination_exists",
            format!(
                "delivery `{}` already has a drawing, evidence directory or reservation",
                target.stem
            ),
        ));
    }
    Ok(target)
}

fn reservation_marker(target: &DeliveryTarget) -> PathBuf {
    target.root.join(format!(".{}.delivery.lock", target.stem))
}

/// Reserve a validated new target using a create-new marker.  The function
/// repeats the existence check because the directory scan and this operation
/// are separate filesystem observations.
pub fn reserve(target: &DeliveryTarget) -> Result<DeliveryReservation, DeliveryNameError> {
    if target.drawing.exists() || target.evidence_dir.exists() {
        return Err(DeliveryNameError::new(
            "delivery_destination_exists",
            format!("delivery `{}` already exists", target.stem),
        ));
    }
    let marker = reservation_marker(target);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(_) => Ok(DeliveryReservation { marker }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(DeliveryNameError::new(
                "delivery_destination_exists",
                format!("delivery `{}` is reserved by another writer", target.stem),
            ))
        }
        Err(error) => Err(DeliveryNameError::new(
            "delivery_reservation_failed",
            format!("cannot reserve delivery `{}`: {error}", target.stem),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ocs-delivery-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn chooses_next_letter_across_files_and_evidence_directories() {
        let root = root("sequence");
        std::fs::write(root.join("YSD06-261020-a.dxf"), b"a").unwrap();
        std::fs::write(root.join("YSD06-261020-b.bak"), b"b").unwrap();
        std::fs::create_dir(root.join("YSD06-261020-c")).unwrap();
        let target = next_target(&root, "261020").unwrap();
        assert_eq!(target.stem, "YSD06-261020-d");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_existing_delivery_even_when_the_generic_save_allows_overwrite() {
        let root = root("collision");
        let path = root.join("YSD06-261020-a.dxf");
        std::fs::write(&path, b"a").unwrap();
        let error = validate_target(&root, &path).unwrap_err();
        assert_eq!(error.code, "delivery_destination_exists");
        let _ = std::fs::remove_dir_all(root);
    }
}
