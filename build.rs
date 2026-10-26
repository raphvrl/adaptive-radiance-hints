use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    compile_shader("src/shaders/geometry.wesl", "geometry.wgsl")?;
    compile_shader("src/shaders/selection.wesl", "selection.wgsl")?;

    Ok(())
}

fn compile_shader(source: &str, output: &str) -> Result<(), Box<dyn Error>> {
    let shader = wesl::Compiler::default()
        .compile(source)
        .inspect_err(|error| eprintln!("{error}"))?;

    shader.emit_rerun_if_changed();

    let output_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    shader.write_to_file(output_dir.join(output))?;

    Ok(())
}
