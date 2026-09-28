#[cfg(test)]
#[path = "fixtures/tests.rs"]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

const SMALL_CLOSURE_MODULES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClosureScope {
    Small,
    AllReachable,
}

impl ClosureScope {
    pub(super) const ALL: [Self; 2] = [Self::Small, Self::AllReachable];

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::AllReachable => "all-reachable",
        }
    }

    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "small" => Ok(Self::Small),
            "all-reachable" => Ok(Self::AllReachable),
            _ => Err(format!(
                "unknown closure scope {value}; expected small or all-reachable"
            )),
        }
    }
}

pub(super) struct ScalingFixture {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) entry_closure_modules: usize,
}

pub(super) fn build_fixture(
    repository_root: &Path,
    source_modules: usize,
    scope: ClosureScope,
) -> Result<ScalingFixture, String> {
    if source_modules < SMALL_CLOSURE_MODULES {
        return Err(format!(
            "source module count {source_modules} is smaller than the required small closure"
        ));
    }
    let entry_closure_modules = match scope {
        ClosureScope::Small => SMALL_CLOSURE_MODULES,
        ClosureScope::AllReachable => source_modules,
    };
    let name = format!("scale-{source_modules}-{}", scope.as_str());
    let path = repository_root
        .join(".tmp")
        .join("benchmark")
        .join("workspace-scaling-fixtures")
        .join(name.as_str());
    write_fixture(path.as_path(), source_modules, entry_closure_modules)?;
    Ok(ScalingFixture {
        name,
        path,
        entry_closure_modules,
    })
}

fn write_fixture(
    root: &Path,
    source_modules: usize,
    entry_closure_modules: usize,
) -> Result<(), String> {
    let source_directory = root.join("src");
    fs::create_dir_all(source_directory.as_path())
        .map_err(|error| format!("could not create scaling fixture: {error}"))?;
    fs::write(
        root.join("galfus.toml"),
        "[module]\nname = \"workspace-scaling\"\ntarget = \"app\"\n[entry]\npath = \"src/main.gfs\"\n",
    )
    .map_err(|error| format!("could not write scaling manifest: {error}"))?;
    fs::write(
        source_directory.join("main.gfs"),
        "import { value_00000 } from \"./module_00000.gfs\"\nexport fn main(args: [[u8]]): i32 { return value_00000() }\n",
    )
    .map_err(|error| format!("could not write scaling entry: {error}"))?;

    let reachable_modules = entry_closure_modules - 1;
    for module_index in 0..source_modules - 1 {
        let source = module_source(module_index, reachable_modules);
        fs::write(
            source_directory.join(format!("module_{module_index:05}.gfs")),
            source,
        )
        .map_err(|error| format!("could not write scaling module {module_index}: {error}"))?;
    }
    Ok(())
}

fn module_source(module_index: usize, reachable_modules: usize) -> String {
    let children = [module_index * 2 + 1, module_index * 2 + 2]
        .into_iter()
        .filter(|child| *child < reachable_modules)
        .collect::<Vec<_>>();
    let imports = children
        .iter()
        .map(|child| format!("import {{ value_{child:05} }} from \"./module_{child:05}.gfs\"\n"))
        .collect::<String>();
    let value = match children.as_slice() {
        [] => "1".to_string(),
        [child] => format!("value_{child:05}()"),
        [left, right] => format!("value_{left:05}() + value_{right:05}()"),
        _ => unreachable!("binary fixture nodes have at most two children"),
    };
    format!("{imports}export fn value_{module_index:05}(): i32 {{ return {value} }}\n")
}
