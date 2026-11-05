use std::{error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let passes = Path::new("src/shaders/passes");
    let output = std::env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?;

    compile_passes(passes, passes, Path::new(&output))
}

fn compile_passes(directory: &Path, root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={}", directory.display());

    for entry in fs::read_dir(directory)? {
        let path = entry?.path();

        if path.is_dir() {
            compile_passes(&path, root, output)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "wesl")
        {
            let relative = path.strip_prefix(root)?.with_extension("");
            let module = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("::");

            let module_path =
                format!("package::passes::{module}").parse::<wesl::syntax::ModulePath>()?;

            let shader = wesl::Compiler::default()
                .compile_module("src/shaders", &module_path)
                .inspect_err(|error| eprintln!("{error}"))?;

            shader.emit_rerun_if_changed();

            let destination = output.join(relative).with_extension("wgsl");
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }

            shader.write_to_file(destination)?;
        }
    }

    Ok(())
}
