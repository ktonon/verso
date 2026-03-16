use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_FILENAME: &str = "verso.schema.json";
pub const SCHEMA_REF: &str = "./verso.schema.json";
pub const SCHEMA_CONTENT: &str = include_str!("../../schema/v0.1.0/verso.schema.json");

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VersoConfig {
    /// JSON Schema reference
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    /// Version of verso that last successfully processed this config
    #[serde(default)]
    pub verso: Option<String>,
    #[serde(default)]
    pub output_directory: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub papers: Option<Vec<PaperConfig>>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaperConfig {
    pub input: String,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug)]
pub struct ResolvedPaper {
    pub input: String,
    pub output: String,
}

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    /// Both `input` and `papers` are specified
    InputAndPapers,
    /// Neither `input` nor `papers` is specified
    NoInput,
    /// An output field contains a file extension
    OutputHasExtension(String),
    /// JSON parse error
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InputAndPapers => {
                write!(f, "config must specify either 'input' or 'papers', not both")
            }
            ConfigError::NoInput => {
                write!(f, "config must specify either 'input' or 'papers'")
            }
            ConfigError::OutputHasExtension(name) => {
                write!(f, "output '{}' must not contain a file extension", name)
            }
            ConfigError::Parse(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

/// Strip single-line comments (`//`) from JSONC content, preserving strings.
pub fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    out.push(next);
                    chars.next();
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' {
            if chars.peek() == Some(&'/') {
                // Consume rest of line
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Derive the default output name from an input path: the file stem.
fn default_output(input: &str) -> String {
    Path::new(input)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

impl VersoConfig {
    /// The output directory, defaulting to `"."`.
    pub fn output_dir(&self) -> &str {
        self.output_directory.as_deref().unwrap_or(".")
    }

    pub fn from_jsonc(text: &str) -> Result<Self, ConfigError> {
        let json = strip_jsonc_comments(text);
        serde_json::from_str(&json).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Validate the config and resolve into a list of papers with concrete output names.
    pub fn resolve(&self) -> Result<Vec<ResolvedPaper>, ConfigError> {
        match (&self.input, &self.papers) {
            (Some(_), Some(_)) => return Err(ConfigError::InputAndPapers),
            (None, None) => return Err(ConfigError::NoInput),
            _ => {}
        }

        let raw: Vec<(&str, Option<&str>)> = if let Some(input) = &self.input {
            vec![(input.as_str(), None)]
        } else {
            self.papers
                .as_ref()
                .unwrap()
                .iter()
                .map(|p| (p.input.as_str(), p.output.as_deref()))
                .collect()
        };

        let mut resolved = Vec::with_capacity(raw.len());
        for (input, output) in raw {
            let out = match output {
                Some(o) => {
                    if Path::new(o).extension().is_some() {
                        return Err(ConfigError::OutputHasExtension(o.to_string()));
                    }
                    o.to_string()
                }
                None => default_output(input),
            };
            resolved.push(ResolvedPaper {
                input: input.to_string(),
                output: out,
            });
        }
        Ok(resolved)
    }
}

/// Generate the default `.verso.jsonc` content for `verso init`.
pub fn default_config_content() -> String {
    format!(
        r#"{{
  "$schema": "{}",
  "verso": "{}",

  // Output directory for build artifacts
  "outputDirectory": "build",

  // Single paper — specify the main .verso file
  "input": "paper.verso"

  // For multiple papers, replace "input" above with "papers":
  // "papers": [
  //   {{
  //     "input": "paper.verso",
  //     "output": "paper"
  //   }},
  //   {{
  //     "input": "other.verso"
  //   }}
  // ]
}}
"#,
        SCHEMA_REF, VERSION
    )
}

/// A loaded and resolved config with all fields ready to use.
pub struct ResolvedConfig {
    pub output_dir: String,
    pub papers: Vec<ResolvedPaper>,
}

impl ResolvedConfig {
    /// Input paths for all papers.
    pub fn inputs(&self) -> Vec<String> {
        self.papers.iter().map(|p| p.input.clone()).collect()
    }
}

/// Write the embedded schema file next to the config file.
pub fn install_schema(config_path: &Path) -> Result<(), ConfigError> {
    let dir = config_path.parent().unwrap_or(Path::new("."));
    let schema_path = dir.join(SCHEMA_FILENAME);
    std::fs::write(&schema_path, SCHEMA_CONTENT)
        .map_err(|e| ConfigError::Parse(format!("writing {}: {}", schema_path.display(), e)))
}

/// Update the `"$schema"` and `"verso"` fields in the config file,
/// and install the schema file alongside it.
pub fn stamp_config(config_path: &Path) -> Result<(), ConfigError> {
    let text = std::fs::read_to_string(config_path)
        .map_err(|e| ConfigError::Parse(format!("reading {}: {}", config_path.display(), e)))?;

    let updated = stamp_config_text(&text);

    std::fs::write(config_path, &updated)
        .map_err(|e| ConfigError::Parse(format!("writing {}: {}", config_path.display(), e)))?;

    install_schema(config_path)
}

fn stamp_config_text(text: &str) -> String {
    let stamps: &[(&str, &str)] = &[("$schema", SCHEMA_REF), ("verso", VERSION)];
    let mut result = text.to_string();
    for &(key, value) in stamps {
        result = stamp_field(&result, key, value);
    }
    result
}

/// Replace or insert a single JSON field in the text.
fn stamp_field(text: &str, key: &str, value: &str) -> String {
    let pattern = format!("\"{}\"", key);

    // Try to find and replace existing field
    let mut found = false;
    for line in text.lines() {
        if line.trim().starts_with(&pattern) && line.contains(':') {
            found = true;
            break;
        }
    }

    if found {
        let mut result = String::with_capacity(text.len());
        for (i, line) in text.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            let trimmed = line.trim();
            if trimmed.starts_with(&pattern) && trimmed.contains(':') {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                let has_comma = trimmed.ends_with(',');
                let comma = if has_comma { "," } else { "" };
                result.push_str(&format!("{}\"{}\": \"{}\"{}", indent, key, value, comma));
            } else {
                result.push_str(line);
            }
        }
        if text.ends_with('\n') {
            result.push('\n');
        }
        result
    } else {
        // Insert after opening brace
        let mut result = String::with_capacity(text.len() + 50);
        let mut inserted = false;
        for (i, line) in text.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            result.push_str(line);
            if !inserted && line.trim() == "{" {
                result.push_str(&format!("\n  \"{}\": \"{}\",", key, value));
                inserted = true;
            }
        }
        if text.ends_with('\n') {
            result.push('\n');
        }
        result
    }
}

pub const CONFIG_FILENAMES: &[&str] = &[".verso.jsonc", ".verso.json"];
pub const CONFIG_FILENAME: &str = ".verso.jsonc";

/// Search for a config file in the given directory.
/// Checks `.verso.jsonc` first, then `.verso.json`.
pub fn find_config(dir: &Path) -> Option<PathBuf> {
    for name in CONFIG_FILENAMES {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Find, load, and resolve the config in the given directory.
/// Returns `None` if no config file exists.
pub fn resolve_config(dir: &Path) -> Result<Option<ResolvedConfig>, ConfigError> {
    let path = match find_config(dir) {
        Some(p) => p,
        None => return Ok(None),
    };
    let config = load_config(&path)?;
    let papers = config.resolve()?;
    Ok(Some(ResolvedConfig {
        output_dir: config.output_dir().to_string(),
        papers,
    }))
}

/// Read and parse a config file (supports JSONC comments).
pub fn load_config(path: &Path) -> Result<VersoConfig, ConfigError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::Parse(format!("reading {}: {}", path.display(), e)))?;
    VersoConfig::from_jsonc(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_input() {
        let cfg = VersoConfig::from_jsonc(r#"{
            "outputDirectory": "build",
            "input": "src/paper.verso"
        }"#)
        .unwrap();
        assert_eq!(cfg.output_directory, Some("build".into()));
        assert_eq!(cfg.input, Some("src/paper.verso".into()));
        assert!(cfg.papers.is_none());
    }

    #[test]
    fn parse_multiple_papers() {
        let cfg = VersoConfig::from_jsonc(r#"{
            "outputDirectory": "out",
            "papers": [
                { "input": "a.verso", "output": "alpha" },
                { "input": "b.verso" }
            ]
        }"#)
        .unwrap();
        assert!(cfg.input.is_none());
        let papers = cfg.papers.unwrap();
        assert_eq!(papers.len(), 2);
        assert_eq!(papers[0].output, Some("alpha".into()));
        assert_eq!(papers[1].output, None);
    }

    #[test]
    fn strip_comments() {
        let input = r#"{
            // This is a comment
            "key": "value" // trailing comment
        }"#;
        let stripped = strip_jsonc_comments(input);
        let v: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(v["key"], "value");
    }

    #[test]
    fn strip_comments_preserves_url_in_string() {
        let input = r#"{ "url": "https://example.com" }"#;
        let stripped = strip_jsonc_comments(input);
        let v: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(v["url"], "https://example.com");
    }

    #[test]
    fn resolve_single_input_defaults_output() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: None,
            input: Some("src/paper.verso".into()),
            papers: None,
        };
        let papers = cfg.resolve().unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].input, "src/paper.verso");
        assert_eq!(papers[0].output, "paper");
    }

    #[test]
    fn resolve_papers_defaults_output_to_stem() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: None,
            input: None,
            papers: Some(vec![
                PaperConfig {
                    input: "src/gateway-sw.verso".into(),
                    output: None,
                },
                PaperConfig {
                    input: "src/main.verso".into(),
                    output: Some("thesis".into()),
                },
            ]),
        };
        let papers = cfg.resolve().unwrap();
        assert_eq!(papers[0].output, "gateway-sw");
        assert_eq!(papers[1].output, "thesis");
    }

    #[test]
    fn resolve_rejects_both_input_and_papers() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: None,
            input: Some("a.verso".into()),
            papers: Some(vec![]),
        };
        assert_eq!(cfg.resolve().unwrap_err(), ConfigError::InputAndPapers);
    }

    #[test]
    fn resolve_rejects_neither_input_nor_papers() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: None,
            input: None,
            papers: None,
        };
        assert_eq!(cfg.resolve().unwrap_err(), ConfigError::NoInput);
    }

    #[test]
    fn resolve_rejects_output_with_extension() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: None,
            input: None,
            papers: Some(vec![PaperConfig {
                input: "a.verso".into(),
                output: Some("a.pdf".into()),
            }]),
        };
        assert_eq!(
            cfg.resolve().unwrap_err(),
            ConfigError::OutputHasExtension("a.pdf".into())
        );
    }

    #[test]
    fn output_dir_defaults_to_dot() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: None,
            input: Some("paper.verso".into()),
            papers: None,
        };
        assert_eq!(cfg.output_dir(), ".");
    }

    #[test]
    fn output_dir_uses_configured_value() {
        let cfg = VersoConfig {
            schema: None,
            verso: None,
            output_directory: Some("build".into()),
            input: Some("paper.verso".into()),
            papers: None,
        };
        assert_eq!(cfg.output_dir(), "build");
    }

    #[test]
    fn find_config_prefers_jsonc() {
        let dir = std::env::temp_dir().join("verso-test-find-config");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".verso.jsonc"), "{}").unwrap();
        std::fs::write(dir.join(".verso.json"), "{}").unwrap();
        let found = find_config(&dir).unwrap();
        assert_eq!(found.file_name().unwrap(), ".verso.jsonc");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_config_falls_back_to_json() {
        let dir = std::env::temp_dir().join("verso-test-find-json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".verso.json"), "{}").unwrap();
        let found = find_config(&dir).unwrap();
        assert_eq!(found.file_name().unwrap(), ".verso.json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_config_returns_none_when_missing() {
        let dir = std::env::temp_dir().join("verso-test-find-none");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(find_config(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_config_reads_jsonc_file() {
        let dir = std::env::temp_dir().join("verso-test-load");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".verso.jsonc");
        std::fs::write(&path, r#"{
            // comment
            "outputDirectory": "dist",
            "input": "main.verso"
        }"#).unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.output_directory, Some("dist".into()));
        assert_eq!(cfg.input, Some("main.verso".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_config_is_valid_jsonc() {
        let content = default_config_content();
        let cfg = VersoConfig::from_jsonc(&content).unwrap();
        assert_eq!(cfg.schema, Some(SCHEMA_REF.into()));
        assert_eq!(cfg.verso, Some(VERSION.into()));
        assert_eq!(cfg.output_directory, Some("build".into()));
        assert_eq!(cfg.input, Some("paper.verso".into()));
    }

    #[test]
    fn stamp_replaces_existing_fields() {
        let input = "{\n  \"$schema\": \"old\",\n  \"verso\": \"0.0.1\",\n  \"input\": \"paper.verso\"\n}\n";
        let result = stamp_config_text(input);
        assert!(result.contains(&format!("\"$schema\": \"{}\"", SCHEMA_REF)));
        assert!(result.contains(&format!("\"verso\": \"{}\"", VERSION)));
        assert!(!result.contains("0.0.1"));
        assert!(!result.contains("\"old\""));
        VersoConfig::from_jsonc(&result).unwrap();
    }

    #[test]
    fn stamp_inserts_when_missing() {
        let input = "{\n  \"input\": \"paper.verso\"\n}\n";
        let result = stamp_config_text(input);
        assert!(result.contains(&format!("\"$schema\": \"{}\"", SCHEMA_REF)));
        assert!(result.contains(&format!("\"verso\": \"{}\"", VERSION)));
        VersoConfig::from_jsonc(&result).unwrap();
    }

    #[test]
    fn config_error_display_input_and_papers() {
        assert_eq!(
            format!("{}", ConfigError::InputAndPapers),
            "config must specify either 'input' or 'papers', not both"
        );
    }

    #[test]
    fn config_error_display_no_input() {
        assert_eq!(
            format!("{}", ConfigError::NoInput),
            "config must specify either 'input' or 'papers'"
        );
    }

    #[test]
    fn config_error_display_output_has_extension() {
        let s = format!("{}", ConfigError::OutputHasExtension("out.pdf".into()));
        assert!(s.contains("out.pdf"));
        assert!(s.contains("extension"));
    }

    #[test]
    fn config_error_display_parse() {
        let s = format!("{}", ConfigError::Parse("bad json".into()));
        assert!(s.contains("bad json"));
    }

    #[test]
    fn strip_comments_escaped_quote_in_string() {
        let input = r#"{ "key": "value with \" escaped" }"#;
        let stripped = strip_jsonc_comments(input);
        let v: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(v["key"], "value with \" escaped");
    }

    #[test]
    fn strip_comments_no_comments() {
        let input = r#"{ "key": "value" }"#;
        let stripped = strip_jsonc_comments(input);
        assert_eq!(stripped, input);
    }

    #[test]
    fn from_jsonc_invalid_json() {
        let result = VersoConfig::from_jsonc("not json at all");
        assert!(matches!(result, Err(ConfigError::Parse(_))));
    }

    #[test]
    fn resolved_config_inputs() {
        let config = ResolvedConfig {
            output_dir: ".".to_string(),
            papers: vec![
                ResolvedPaper {
                    input: "a.verso".to_string(),
                    output: "a".to_string(),
                },
                ResolvedPaper {
                    input: "b.verso".to_string(),
                    output: "b".to_string(),
                },
            ],
        };
        assert_eq!(config.inputs(), vec!["a.verso", "b.verso"]);
    }

    #[test]
    fn stamp_preserves_comments() {
        let input = "{\n  // A comment\n  \"verso\": \"0.0.1\",\n  \"input\": \"paper.verso\"\n}\n";
        let result = stamp_config_text(input);
        assert!(result.contains("// A comment"));
        assert!(result.contains(&format!("\"$schema\": \"{}\"", SCHEMA_REF)));
        assert!(result.contains(&format!("\"verso\": \"{}\"", VERSION)));
    }
}
