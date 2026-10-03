//! Finding severity levels.

use std::fmt;
use std::str::FromStr;

/// How serious a finding is. Ordered `Log < Warning < Error` so thresholds
/// (`--min-severity`, `--fail-on`) can be expressed as simple comparisons.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — surfaced but not intended to gate CI by default.
    Log,
    /// A convention or quality violation.
    Warning,
    /// A likely defect or security issue.
    Error,
}

impl Severity {
    /// The default severity applied to a pattern that omits `(#set! severity)`.
    pub const DEFAULT: Severity = Severity::Warning;

    /// Lowercase name, matching the `#set! severity` values and JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Log => "log",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "log" | "info" | "note" => Ok(Severity::Log),
            "warning" | "warn" => Ok(Severity::Warning),
            "error" | "err" => Ok(Severity::Error),
            other => Err(format!(
                "unknown severity {other:?} (expected log, warning, or error)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_log_lt_warning_lt_error() {
        assert!(Severity::Log < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn parses_aliases() {
        assert_eq!("warn".parse::<Severity>().unwrap(), Severity::Warning);
        assert_eq!("ERROR".parse::<Severity>().unwrap(), Severity::Error);
        assert!("bogus".parse::<Severity>().is_err());
    }
}
