//! Response formatting layer.
//!
//! Provides a [`ResponseFormat`] enum for selecting between markdown and JSON
//! output, plus per-domain free functions for rendering rich markdown.
//!
//! # Design
//!
//! - **No trait.** Two concrete render paths only:
//!   - `render_<domain>_<view>_markdown(&Value) -> String` — domain-specific markdown.
//!   - `serde_json::to_string_pretty(&value)` — JSON passthrough (no wrapper).
//! - **No cache.** JSON serialization is cheap; markdown is not in a hot loop.
//! - **`&serde_json::Value` inputs.** Service methods return `Result<Value>`; renderers
//!   consume the value by reference so the caller can still handle JSON mode.
//!
//! # Per-domain modules
//!
//! - [`container`] — `render_container_*_markdown`
//! - [`compose`]   — `render_compose_*_markdown`
//! - [`docker`]    — `render_docker_*_markdown`
//! - [`host`]      — `render_host_*_markdown`
//! - [`scout`]     — `render_scout_*_markdown`

pub mod compose;
pub mod container;
pub mod docker;
pub mod host;
pub mod scout;

// Unit tests live in a sidecar file — see src/formatters_tests.rs.
#[cfg(test)]
#[path = "formatters_tests.rs"]
mod tests;

/// Output format requested by the caller.
///
/// Parse from a JSON string arg with [`ResponseFormat::parse`] (shim-layer
/// responsibility). Absence of the arg → [`ResponseFormat::Markdown`] default.
/// An unrecognised string is an error — never a silent fallback.
///
/// # Examples
///
/// ```rust
/// use synapse2::formatters::ResponseFormat;
///
/// assert_eq!(ResponseFormat::parse(None).unwrap(), ResponseFormat::Markdown);
/// assert_eq!(ResponseFormat::parse(Some("json")).unwrap(), ResponseFormat::Json);
/// assert_eq!(ResponseFormat::parse(Some("markdown")).unwrap(), ResponseFormat::Markdown);
/// assert!(ResponseFormat::parse(Some("xml")).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    /// Rich markdown output (default).
    Markdown,
    /// Pretty-printed JSON (`serde_json::to_string_pretty`).
    Json,
}

impl ResponseFormat {
    /// Parse an optional string argument into a `ResponseFormat`.
    ///
    /// - `None` → `Ok(Markdown)` (the default)
    /// - `Some("markdown")` → `Ok(Markdown)`
    /// - `Some("json")` → `Ok(Json)`
    /// - any other value → `Err` with a clear message
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Markdown),
            Some(s) => match s.trim().to_ascii_lowercase().as_str() {
                "markdown" | "md" => Ok(Self::Markdown),
                "json" => Ok(Self::Json),
                other => Err(format!(
                    "unknown response_format {other:?}; expected \"markdown\" or \"json\""
                )),
            },
        }
    }

    /// Returns `true` if this is the JSON variant.
    #[inline]
    pub fn is_json(self) -> bool {
        self == Self::Json
    }

    /// Render a `serde_json::Value` according to the format.
    ///
    /// - `Markdown` → delegates to the provided closure.
    /// - `Json` → `serde_json::to_string_pretty`.
    ///
    /// This helper lets call sites stay concise:
    ///
    /// ```rust,ignore
    /// let out = fmt.render(&value, |v| render_container_list_markdown(v));
    /// ```
    pub fn render<F>(&self, value: &serde_json::Value, markdown_fn: F) -> String
    where
        F: FnOnce(&serde_json::Value) -> String,
    {
        match self {
            Self::Markdown => markdown_fn(value),
            Self::Json => serde_json::to_string_pretty(value)
                .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        }
    }
}

/// Return the current UTC time formatted per STYLE.md §3.6.
///
/// Format: `As of (UTC): HH:MM:SS | MM/DD/YYYY`
pub(crate) fn format_timestamp() -> String {
    use chrono::Utc;
    let now = Utc::now();
    format!(
        "As of (UTC): {} | {}",
        now.format("%H:%M:%S"),
        now.format("%m/%d/%Y")
    )
}

/// Format a byte count into a compact human-readable string.
///
/// Follows the SI prefix convention used in synapse-mcp (e.g. `1.5 GB`).
pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Truncate a string to at most `max_chars` characters, appending `…` if truncated.
pub(crate) fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        let truncated: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        format!("{truncated}…")
    }
}

/// Get a string field from a JSON Value, returning `"—"` when absent.
pub(crate) fn str_field<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|f| f.as_str()).unwrap_or("—")
}


