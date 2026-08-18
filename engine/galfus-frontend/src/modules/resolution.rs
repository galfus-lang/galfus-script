use galfus_core::ModulePath;

pub fn is_resolvable_import(
    source: &str,
    catalog: Option<&galfus_contract::CapabilityCatalog>,
) -> bool {
    source.starts_with("./")
        || source.starts_with("../")
        || is_builtin_module(source)
        || catalog.is_some_and(|c| c.is_provider_module(source))
}

pub fn is_relative_import(source: &str) -> bool {
    source.starts_with("./") || source.starts_with("../")
}

pub fn is_builtin_module(source: &str) -> bool {
    galfus_contract::is_builtin_module(source)
}

// In the new architecture, the module path resolution logic belongs in the frontend,
// because it affects semantic module identity.
pub fn resolve_relative_import(
    base_module: &ModulePath,
    source: &str,
    catalog: Option<&galfus_contract::CapabilityCatalog>,
) -> Option<ModulePath> {
    if is_builtin_module(source) || catalog.is_some_and(|c| c.is_provider_module(source)) {
        return ModulePath::new(format!("{source}.gfs").as_str());
    }

    // Example: base_module = "src/main.gfs", source = "./utils" -> "src/utils.gfs"
    let base_str = base_module.as_str();
    let mut segments: Vec<&str> = base_str.split('/').collect();

    // Remove the file part
    if !segments.is_empty() {
        segments.pop();
    }

    let source_segments: Vec<&str> = source.split('/').collect();
    for segment in source_segments {
        if segment == "." || segment.is_empty() {
            continue;
        } else if segment == ".." {
            if segments.is_empty() {
                return None;
            }
            segments.pop();
        } else {
            segments.push(segment);
        }
    }

    let mut path = segments.join("/");
    if !path.ends_with(".gfs") && !path.ends_with(".gfp") {
        path.push_str(".gfs");
    }

    ModulePath::new(&path)
}
