use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Rerun if frontend sources change
    println!("cargo:rerun-if-changed=../frontend/src");
    println!("cargo:rerun-if-changed=../frontend/Cargo.toml");
    println!("cargo:rerun-if-changed=../frontend/Trunk.toml");
    println!("cargo:rerun-if-changed=../frontend/index.html");
    println!("cargo:rerun-if-changed=../frontend/style");

    // Check if trunk is installed
    let trunk_check = Command::new("trunk").arg("--version").output();

    if trunk_check.is_err() {
        eprintln!("\n{}", "=".repeat(80));
        eprintln!("ERROR: 'trunk' is not installed or not found in PATH");
        eprintln!("{}", "=".repeat(80));
        eprintln!("\nThe API build requires 'trunk' to build the frontend.");
        eprintln!("\nTo install trunk, run:");
        eprintln!("  cargo install trunk");
        eprintln!("\nOr visit: https://trunkrs.dev");
        eprintln!("{}\n", "=".repeat(80));
        std::process::exit(1);
    }

    // Get workspace root (parent of api directory)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir).parent().unwrap().to_path_buf();

    // Map cargo profile to trunk profile ("debug" -> "dev")
    let profile = env::var("PROFILE").unwrap();
    let trunk_profile = if profile == "debug" { "dev" } else { &profile };

    println!("Building frontend...");

    let frontend_dir = workspace_root.join("frontend");
    let frontend_target_dir = workspace_root.join("target/frontend");

    let result = Command::new("trunk")
        .arg("build")
        .arg("--cargo-profile")
        .arg(trunk_profile)
        .env("CARGO_TARGET_DIR", &frontend_target_dir)
        .current_dir(&frontend_dir)
        .output();

    match result {
        Ok(output) if output.status.success() => {
            println!("Frontend built successfully");
        }
        Ok(output) => {
            eprintln!("\n{}", "=".repeat(80));
            eprintln!("ERROR: Failed to build frontend");
            eprintln!("{}", "=".repeat(80));
            eprintln!("\nStdout:");
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            eprintln!("\nStderr:");
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            eprintln!("{}\n", "=".repeat(80));
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("\n{}", "=".repeat(80));
            eprintln!("ERROR: Failed to execute trunk");
            eprintln!("{}", "=".repeat(80));
            eprintln!("\nError: {}", e);
            eprintln!("{}\n", "=".repeat(80));
            std::process::exit(1);
        }
    }
}
