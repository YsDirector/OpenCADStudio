//! Pure save-destination checks shared by the native delivery paths.
//!
//! A successful CAD serialization is not enough to make an output safe to
//! hand to another person.  Check the destination before preparing a
//! snapshot: absolute path, supported extension, existing parent directory,
//! and explicit overwrite acknowledgement all belong to this boundary.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadOutputFormat {
    Dwg,
    Dxf,
}

impl CadOutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Dwg => "dwg",
            Self::Dxf => "dxf",
        }
    }

    pub fn from_extension(path: &Path) -> Result<Self, OutputPreflightError> {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("dwg") => Ok(Self::Dwg),
            Some("dxf") => Ok(Self::Dxf),
            Some(ext) => Err(OutputPreflightError::new(
                "unsupported_format",
                format!("output path must end in .dwg or .dxf, not .{ext}"),
            )),
            None => Err(OutputPreflightError::new(
                "unsupported_format",
                "output path must end in .dwg or .dxf",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPreflight {
    pub path: PathBuf,
    pub format: CadOutputFormat,
    pub existed: bool,
    pub overwriting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPreflightError {
    pub code: &'static str,
    pub message: String,
}

impl OutputPreflightError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for OutputPreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for OutputPreflightError {}

/// Validate a native CAD output destination without opening or mutating it.
///
/// The check intentionally uses `symlink_metadata` for an existing target:
/// delivery must not silently follow a symlink and replace a different file.
/// A caller that is allowed to overwrite must say so explicitly.
pub fn preflight_cad_output(
    path: &Path,
    overwrite: bool,
) -> Result<OutputPreflight, OutputPreflightError> {
    if !path.is_absolute() {
        return Err(OutputPreflightError::new(
            "absolute_path_required",
            "output path must be absolute",
        ));
    }
    let format = CadOutputFormat::from_extension(path)?;
    if path.file_name().is_none() {
        return Err(OutputPreflightError::new(
            "path_required",
            "output path must name a file",
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        OutputPreflightError::new("parent_required", "output path has no parent directory")
    })?;
    if !parent.is_dir() {
        return Err(OutputPreflightError::new(
            "parent_missing",
            format!(
                "output parent directory does not exist: {}",
                parent.display()
            ),
        ));
    }

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(OutputPreflightError::new(
                "destination_stat_failed",
                format!("cannot inspect output destination: {error}"),
            ))
        }
    };
    let existed = metadata.is_some();
    if let Some(metadata) = metadata {
        if metadata.file_type().is_symlink() {
            return Err(OutputPreflightError::new(
                "destination_symlink",
                "refusing to write through a symlink destination",
            ));
        }
        if metadata.is_dir() {
            return Err(OutputPreflightError::new(
                "destination_is_directory",
                "output destination is a directory",
            ));
        }
        if !overwrite {
            return Err(OutputPreflightError::new(
                "destination_exists",
                "destination exists; set overwrite=true to replace it",
            ));
        }
    }
    Ok(OutputPreflight {
        path: path.to_path_buf(),
        format,
        existed,
        overwriting: existed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ocs-output-preflight-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn rejects_relative_and_unsupported_paths() {
        assert_eq!(
            preflight_cad_output(Path::new("drawing.dxf"), false)
                .unwrap_err()
                .code,
            "absolute_path_required"
        );
        let path = temp_path("drawing.pdf");
        assert_eq!(
            preflight_cad_output(&path, false).unwrap_err().code,
            "unsupported_format"
        );
    }

    #[test]
    fn requires_explicit_overwrite() {
        let path = temp_path("exists.dxf");
        std::fs::write(&path, b"old").unwrap();
        assert_eq!(
            preflight_cad_output(&path, false).unwrap_err().code,
            "destination_exists"
        );
        let plan = preflight_cad_output(&path, true).unwrap();
        assert!(plan.existed && plan.overwriting);
        let _ = std::fs::remove_file(path);
    }
}
