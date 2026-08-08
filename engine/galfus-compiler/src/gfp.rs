use galfus_contract::AdapterConfig;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GfpFrontmatter {
    pub adapter: String,
    #[serde(default)]
    pub config: AdapterConfig,
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

    let table: toml::Table = toml::from_str(toml_str)
        .map_err(|err| format!("invalid TOML frontmatter in .gfp: {err}"))?;

    if table.contains_key("targets") || table.contains_key("metadata") {
        return Err("The .gfp frontmatter format has changed. [targets] and [metadata] are no longer supported. Please move your settings into [config].".to_string());
    }

    let frontmatter: GfpFrontmatter = toml::from_str(toml_str)
        .map_err(|err| format!("invalid TOML frontmatter in .gfp: {err}"))?;

    Ok((frontmatter, galfus_code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use galfus_contract::AdapterConfigValue;

    #[test]
    fn test_parse_gfp_frontmatter_with_config() {
        let source = r#"---
adapter = "c_abi"

[config]
thread_affinity = "main"

[config.libraries]
linux = "./bin/libSDL2.so"
windows = "./bin/SDL2.dll"
---

export fn(async) SDL_Init(flags: u32): i32
"#;

        let (frontmatter, galfus_code) = parse_gfp_frontmatter(source).unwrap();
        assert_eq!(frontmatter.adapter, "c_abi");

        let thread_affinity = frontmatter.config.get("thread_affinity").unwrap();
        assert!(matches!(thread_affinity, AdapterConfigValue::String(s) if s == "main"));

        let libraries = match frontmatter.config.get("libraries").unwrap() {
            AdapterConfigValue::Table(t) => t,
            _ => panic!("Expected table"),
        };

        assert!(
            matches!(libraries.get("linux").unwrap(), AdapterConfigValue::String(s) if s == "./bin/libSDL2.so")
        );
        assert!(galfus_code.starts_with("export fn(async) SDL_Init"));
        assert_eq!(
            frontmatter
                .config
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["libraries", "thread_affinity"]
        );
    }

    #[test]
    fn test_reject_legacy_format() {
        let source = r#"---
adapter = "c_abi"

[targets]
windows = "./bin/SDL2.dll"
---
"#;
        let err = parse_gfp_frontmatter(source).unwrap_err();
        assert!(err.contains("[targets] and [metadata] are no longer supported"));
    }
}
