use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

use std::fs;
use std::path::Path;

pub fn run_init() -> Result<()> {
    let theme = ColorfulTheme::default();

    println!("Welcome to Galfus Script! Let's set up your new project.\n");

    let project_name: String = Input::with_theme(&theme)
        .with_prompt("Project name")
        .interact_text()?;

    let slug = slugify(&project_name);

    let version: String = Input::with_theme(&theme)
        .with_prompt("Version")
        .default("0.0.1".to_string())
        .interact_text()?;

    let directory: String = Input::with_theme(&theme)
        .with_prompt("Where should we create it?")
        .default(slug)
        .interact_text()?;

    let path = Path::new(&directory);

    if path.exists() {
        let confirm = Confirm::with_theme(&theme)
            .with_prompt("Directory already exists. Initialize project here anyway?")
            .default(false)
            .interact()?;
        if !confirm {
            println!("Aborted.");
            return Ok(());
        }
    }

    let project_types = &["App (Executable)", "Lib (Library)"];
    let selection = Select::with_theme(&theme)
        .with_prompt("Project type")
        .default(0)
        .items(&project_types[..])
        .interact()?;

    let is_app = selection == 0;

    if !path.exists() {
        println!("\nCreating directory '{}'...", directory);
        fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory {}", directory))?;
    } else {
        println!("\nInitializing in existing directory '{}'...", directory);
    }

    let src_path = path.join("src");
    fs::create_dir_all(&src_path).with_context(|| "Failed to create src directory")?;

    let module_target = if is_app { "app" } else { "lib" };
    let entry_file = if is_app { "main.gfs" } else { "lib.gfs" };
    let entry_path = format!("src/{}", entry_file);

    let toml_content = format!(
        "[module]\nname = \"{}\"\nversion = \"{}\"\ntarget = \"{}\"\n\n[entry]\npath = \"{}\"\n",
        project_name, version, module_target, entry_path
    );

    let toml_path = path.join("galfus.toml");
    fs::write(&toml_path, toml_content).context("Failed to write galfus.toml")?;
    println!("Created galfus.toml");

    let source_code = if is_app {
        "import { println } from \"std/io\"\n\nexport fn main(args: [[u8]]): i32 {\n    println(\"Hello Galfus!\")\n    return 0\n}\n"
    } else {
        "export fn add(a: i32, b: i32): i32 {\n    return a + b\n}\n"
    };

    let main_path = path.join(&entry_path);
    fs::write(&main_path, source_code)
        .with_context(|| format!("Failed to write {}", entry_path))?;
    println!("Created {}", entry_path);

    let gitignore_content = "# output\nbuild/\ndist/\n\n# metadata\n.DS_Store\n";
    let gitignore_path = path.join(".gitignore");
    fs::write(&gitignore_path, gitignore_content).context("Failed to write .gitignore")?;
    println!("Created .gitignore");

    println!("\n🎉 Project '{}' created successfully!", project_name);
    println!("\nTo get started:");

    if directory != "." {
        println!("  cd {}", directory);
    }

    if is_app {
        println!("  galfus run .");
    } else {
        println!("  galfus check .");
    }

    Ok(())
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}
