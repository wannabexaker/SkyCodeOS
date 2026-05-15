use std::process::Command as Proc;

#[derive(Debug, clap::Args)]
pub struct SelfcheckArgs {}

pub fn run(_args: &SelfcheckArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running all-tools smoke suite (phase10d)...");
    let status = Proc::new("cargo")
        .args([
            "test",
            "--manifest-path",
            "runtime/Cargo.toml",
            "phase10d_tools_smoke",
            "--",
            "--test-threads=1",
        ])
        .status()?;

    if !status.success() {
        return Err("selfcheck failed".into());
    }

    println!("selfcheck PASSED");
    Ok(())
}
