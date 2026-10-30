use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    compile_shader("passes::geometry", "geometry.wgsl")?;
    compile_shader("passes::shadow", "shadow.wgsl")?;
    compile_shader("passes::selection", "selection.wgsl")?;
    compile_shader("passes::gizmo", "gizmo.wgsl")?;
    compile_shader("passes::tonemap", "tonemap.wgsl")?;

    Ok(())
}

fn compile_shader(source: &str, output: &str) -> Result<(), Box<dyn Error>> {
    let module_path = format!("package::{source}").parse::<wesl::syntax::ModulePath>()?;
    let shader = wesl::Compiler::default()
        .compile_module("src/shaders", &module_path)
        .inspect_err(|error| eprintln!("{error}"))?;

    shader.emit_rerun_if_changed();

    let output_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    shader.write_to_file(output_dir.join(output))?;

    Ok(())
}
