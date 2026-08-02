use std::collections::HashMap;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GfpFrontmatter {
    pub adapter: String,
    #[serde(default)]
    pub targets: HashMap<String, String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Splits a `.gfp` source text into its TOML frontmatter string and Galfus declarations string.
pub fn parse_gfp_frontmatter(source: &str) -> Result<(GfpFrontmatter, &str), String> {
    let trimmed = source.trim_start();
    if !trimmed.starts_with("---") {
        return Err("missing '---' TOML frontmatter in .gfp file".to_string());
    }

    let after_first_delimiter = &trimmed[3..];
    let end_index = after_first_delimiter
        .find("\n---")
        .ok_or_else(|| "unclosed '---' TOML frontmatter in .gfp file".to_string())?;

    let toml_str = &after_first_delimiter[..end_index];
    let galfus_code = after_first_delimiter[end_index + 4..].trim_start();

    let frontmatter: GfpFrontmatter = toml::from_str(toml_str)
        .map_err(|err| format!("invalid TOML frontmatter in .gfp: {err}"))?;

    Ok((frontmatter, galfus_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gfp_frontmatter() {
        let source = r#"---
adapter = "c_abi"

[targets]
windows = "./bin/SDL2.dll"
linux = "./bin/libSDL2.so"
macos = "./bin/libSDL2.dylib"

[metadata]
thread_affinity = "main"
---

export fn(async) SDL_Init(flags: u32): i32
"#;

        let (frontmatter, galfus_code) = parse_gfp_frontmatter(source).unwrap();
        assert_eq!(frontmatter.adapter, "c_abi");
        assert_eq!(
            frontmatter.targets.get("linux").unwrap(),
            "./bin/libSDL2.so"
        );
        assert_eq!(frontmatter.metadata.get("thread_affinity").unwrap(), "main");
        assert!(galfus_code.starts_with("export fn(async) SDL_Init"));
    }
}
