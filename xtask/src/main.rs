//! Project chores that Cargo cannot do on its own: installing the system
//! packages listed in `.dependencies`, checking the setup, the start-of-day
//! routine and the same checks CI runs.
//!
//! Run from anywhere in the repo:
//!
//! ```text
//! cargo xtask setup | doctor | recap | check | help
//! ```
//!
//! Everyday work stays plain Cargo: `cargo build`, `cargo run -r`, `cargo test`.
//!
//! This program has no dependencies, so it compiles in a few seconds even on a
//! machine where the main crate cannot build yet.

use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

type Result<T = ()> = std::result::Result<T, String>;

const COMMANDS: &[(&str, &str)] = &[
    ("setup", "Install everything listed in .dependencies"),
    (
        "doctor",
        "Check that everything is installed, without changing anything",
    ),
    (
        "recap",
        "Start of the day: latest code, Rust update check, tool check, build",
    ),
    ("check", "Run the same checks as CI: format, lint, test"),
    ("help", "Show this list"),
];

fn main() -> ExitCode {
    if let Err(e) = env::set_current_dir(repo_root()) {
        eprintln!("Cannot open the repo folder: {e}");
        return ExitCode::FAILURE;
    }
    let task = env::args().nth(1).unwrap_or_else(|| "help".to_string());
    let result = match task.as_str() {
        "setup" => setup(),
        "doctor" => doctor(),
        "recap" => recap(),
        "check" => check(),
        "help" | "-h" | "--help" => {
            help();
            Ok(())
        }
        other => Err(format!("Unknown command '{other}'. Run: cargo xtask help")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", paint(Color::Red, &e));
            ExitCode::FAILURE
        }
    }
}

/// The repo root: xtask lives in `<repo>/xtask`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a folder inside the repo")
        .to_path_buf()
}

fn help() {
    println!("Usage: cargo xtask <command>\n");
    for (name, text) in COMMANDS {
        println!("  {name:<8} {text}");
    }
    println!(
        "\nEveryday commands are plain Cargo: cargo build, cargo run -r, cargo test, cargo fmt, cargo lint."
    );
}

// ---------------------------------------------------------------- setup

fn setup() -> Result {
    match env::consts::OS {
        "linux" => setup_linux()?,
        "macos" => setup_macos()?,
        "windows" => setup_windows()?,
        other => {
            return Err(format!(
                "Unsupported system: {other}. Install the packages in .dependencies by hand."
            ));
        }
    }
    setup_rust()?;
    println!(
        "\nSetup done. If anything was installed just now, open a new terminal before you build."
    );
    Ok(())
}

fn setup_linux() -> Result {
    if !on_path("apt-get") {
        return Err("This setup installs packages with apt. Install the [apt] packages in .dependencies with your own package manager.".to_string());
    }
    let packages = first_words(section("apt")?);
    let missing: Vec<&str> = packages
        .iter()
        .map(String::as_str)
        .filter(|p| !apt_installed(p))
        .collect();
    if missing.is_empty() {
        println!("All apt packages are installed.");
        return Ok(());
    }
    run("sudo", &["apt-get", "update"])?;
    let mut args = vec!["apt-get", "install", "-y"];
    args.extend(&missing);
    run("sudo", &args)
}

fn setup_macos() -> Result {
    if !succeeds("xcode-select", &["-p"]) {
        run("xcode-select", &["--install"])?;
        return Err(
            "Finish the Xcode Command Line Tools installer, then run cargo xtask setup again."
                .to_string(),
        );
    }
    let packages = first_words(section("brew")?);
    if packages.is_empty() {
        println!("No Homebrew packages needed.");
        return Ok(());
    }
    if !on_path("brew") {
        return Err("The [brew] packages need Homebrew. Install it from https://brew.sh, then run cargo xtask setup again.".to_string());
    }
    let mut args = vec!["install"];
    args.extend(packages.iter().map(String::as_str));
    run("brew", &args)
}

fn setup_windows() -> Result {
    if !on_path("winget") {
        return Err("winget is missing. Install App Installer from the Microsoft Store, then run cargo xtask setup again.".to_string());
    }
    for (id, installer_options) in winget_entries()? {
        if winget_installed(&id) {
            print_status(true, &id);
            continue;
        }
        println!("{}", paint(Color::Cyan, &format!("  install  {id}")));
        let mut args = vec![
            "install",
            "--id",
            id.as_str(),
            "--exact",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ];
        if let Some(options) = &installer_options {
            args.extend(["--override", options.as_str()]);
        }
        run("winget", &args)?;
    }
    Ok(())
}

/// Installs the toolchain and components from .dependencies. An installed
/// toolchain is left as it is: on Windows, rustup cannot replace cargo.exe
/// while cargo is running this program. `cargo xtask recap` reports updates.
fn setup_rust() -> Result {
    let toolchain = rust_toolchain()?;
    if !toolchain_installed(&toolchain) {
        run(
            "rustup",
            &["toolchain", "install", &toolchain, "--profile", "minimal"],
        )?;
    }
    for component in first_words(section("rust-components")?) {
        if !component_installed(&toolchain, &component) {
            run(
                "rustup",
                &["component", "add", &component, "--toolchain", &toolchain],
            )?;
        }
    }
    let default = output("rustup", &["default"]).unwrap_or_default();
    if !default.starts_with(&toolchain) {
        run("rustup", &["default", &toolchain])?;
    }
    Ok(())
}

// ---------------------------------------------------------------- doctor

fn doctor() -> Result {
    let items = checklist()?;
    for (name, ok) in &items {
        print_status(*ok, name);
    }
    if items.iter().all(|(_, ok)| *ok) {
        println!("Everything is installed.");
        Ok(())
    } else {
        Err("Something is missing. Run: cargo xtask setup".to_string())
    }
}

/// Every item from .dependencies that applies to this system, and whether it is installed.
fn checklist() -> Result<Vec<(String, bool)>> {
    let toolchain = rust_toolchain()?;
    let mut items = vec![(format!("Rust {toolchain}"), toolchain_installed(&toolchain))];
    for component in first_words(section("rust-components")?) {
        let ok = component_installed(&toolchain, &component);
        items.push((component, ok));
    }
    match env::consts::OS {
        "linux" if on_path("dpkg") => {
            for package in first_words(section("apt")?) {
                let ok = apt_installed(&package);
                items.push((package, ok));
            }
        }
        "macos" => {
            items.push((
                "Xcode Command Line Tools".to_string(),
                succeeds("xcode-select", &["-p"]),
            ));
            for package in first_words(section("brew")?) {
                let ok = succeeds("brew", &["list", &package]);
                items.push((package, ok));
            }
        }
        "windows" => {
            for (id, _) in winget_entries()? {
                let ok = winget_installed(&id);
                items.push((id, ok));
            }
        }
        _ => {}
    }
    Ok(items)
}

// ---------------------------------------------------------------- recap

/// Start-of-day routine. It only fast-forwards, so it cannot create merge
/// commits or conflicts. Anything that needs a decision is printed instead.
fn recap() -> Result {
    step("Code");
    let branch = output("git", &["branch", "--show-current"]).unwrap_or_default();
    println!("You are on: {branch}");
    if !succeeds("git", &["fetch", "--prune", "--quiet", "origin"]) {
        note("Could not reach GitHub. Continuing with the code you have.");
    }

    let before = output("git", &["rev-parse", "HEAD"]).unwrap_or_default();
    if succeeds("git", &["rev-parse", "--abbrev-ref", "@{u}"]) {
        if !shows_ok("git", &["pull", "--ff-only", "--quiet"]) {
            note(&format!(
                "Could not update {branch}: it has commits that GitHub does not have, and the reverse. Merge or rebase it yourself."
            ));
        }
    } else {
        note(&format!(
            "{branch} is not on GitHub yet. Push it with: git push -u origin {branch}"
        ));
    }

    let after = output("git", &["rev-parse", "HEAD"]).unwrap_or_default();
    if before == after {
        println!("{branch} is up to date.");
    } else {
        println!("New commits on {branch}:");
        shows_ok(
            "git",
            &["--no-pager", "log", "--oneline", &format!("{before}..HEAD")],
        );
        if !succeeds(
            "git",
            &["diff", "--quiet", &before, "HEAD", "--", ".dependencies"],
        ) {
            note(".dependencies changed in the new commits. Run: cargo xtask setup");
        }
    }

    if branch != "main" && succeeds("git", &["rev-parse", "-q", "--verify", "origin/main"]) {
        let behind: u32 = output("git", &["rev-list", "--count", "HEAD..origin/main"])
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if behind > 0 {
            note(&format!(
                "main has {behind} commit(s) that {branch} does not have. To bring them in: git merge origin/main"
            ));
        }
    }

    let changes = output("git", &["status", "--porcelain"])
        .map(|s| s.lines().count())
        .unwrap_or(0);
    if changes > 0 {
        note(&format!(
            "You have {changes} file(s) with uncommitted changes."
        ));
    }

    step("Rust");
    // rustup check exits with a non-zero code when an update exists, so read its output either way.
    match Command::new("rustup")
        .arg("check")
        .stderr(Stdio::null())
        .output()
    {
        Ok(out) => {
            let report = String::from_utf8_lossy(&out.stdout);
            let updates: Vec<&str> = report
                .lines()
                .filter(|l| l.contains("Update available"))
                .collect();
            if updates.is_empty() {
                println!("Rust is up to date.");
            } else {
                for line in updates {
                    note(line.trim());
                }
                note("Run: rustup update");
            }
        }
        Err(_) => note("Could not check for Rust updates."),
    }

    step("Tools");
    if checklist()?.iter().all(|(_, ok)| *ok) {
        println!("Everything in .dependencies is installed.");
    } else {
        note(
            "Something in .dependencies is missing. Run: cargo xtask doctor, then cargo xtask setup",
        );
    }

    step("Build");
    if shows_ok(&cargo(), &["build", "--quiet"]) {
        println!("Build OK.");
    } else {
        note("The build failed. Read the errors above.");
    }

    step("Pull requests");
    if on_path("gh") && succeeds("gh", &["auth", "status"]) {
        shows_ok("gh", &["pr", "status"]);
    } else {
        println!(
            "Install and log in to the GitHub CLI (gh) to see pull requests waiting for your review here."
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- check

fn check() -> Result {
    let cargo = cargo();
    run(&cargo, &["fmt", "--all", "--check"])?;
    run(
        &cargo,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run(&cargo, &["test", "--workspace"])
}

// ---------------------------------------------------------------- .dependencies

/// The items under one [section] of .dependencies, without comments.
fn section(name: &str) -> Result<Vec<String>> {
    let text = fs::read_to_string(".dependencies")
        .map_err(|e| format!("Cannot read .dependencies: {e}"))?;
    let mut inside = false;
    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            inside = header.strip_suffix(']') == Some(name);
            continue;
        }
        let item = line.split('#').next().unwrap_or("").trim();
        if inside && !item.is_empty() {
            items.push(item.to_string());
        }
    }
    Ok(items)
}

fn first_words(items: Vec<String>) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| item.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

fn rust_toolchain() -> Result<String> {
    Ok(first_words(section("rust-toolchain")?)
        .into_iter()
        .next()
        .unwrap_or_else(|| "stable".to_string()))
}

/// Package ID and optional installer options (the text after `|`).
fn winget_entries() -> Result<Vec<(String, Option<String>)>> {
    Ok(section("winget")?
        .into_iter()
        .map(|item| match item.split_once('|') {
            Some((id, options)) => (id.trim().to_string(), Some(options.trim().to_string())),
            None => (item, None),
        })
        .collect())
}

// ---------------------------------------------------------------- what is installed

fn apt_installed(package: &str) -> bool {
    succeeds("dpkg", &["-s", package])
}

fn toolchain_installed(toolchain: &str) -> bool {
    output("rustup", &["toolchain", "list"]).is_some_and(|list| {
        list.lines()
            .any(|l| l == toolchain || l.starts_with(&format!("{toolchain}-")))
    })
}

fn component_installed(toolchain: &str, component: &str) -> bool {
    output(
        "rustup",
        &["component", "list", "--installed", "--toolchain", toolchain],
    )
    .is_some_and(|list| {
        list.lines()
            .any(|l| l.starts_with(&format!("{component}-")))
    })
}

/// These count as installed however they got there, not only when winget
/// installed them. Without this, setup would install a second copy next to
/// one installed by hand, for example Build Tools next to Visual Studio Community.
fn winget_installed(id: &str) -> bool {
    match id {
        "Git.Git" => on_path("git"),
        "Rustlang.Rustup" => on_path("rustup"),
        "StrawberryPerl.StrawberryPerl" => on_path("perl"),
        "Microsoft.VisualStudio.2022.BuildTools" => msvc_installed(),
        _ => succeeds(
            "winget",
            &["list", "--id", id, "--exact", "--accept-source-agreements"],
        ),
    }
}

/// True when any Visual Studio edition has the C++ compiler, found with vswhere.
fn msvc_installed() -> bool {
    let Some(program_files) = env::var_os("ProgramFiles(x86)") else {
        return false;
    };
    let vswhere = PathBuf::from(program_files)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    output(
        &vswhere.to_string_lossy(),
        &[
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ],
    )
    .is_some_and(|path| !path.is_empty())
}

/// Whether a program can be found on PATH.
fn on_path(name: &str) -> bool {
    let extensions: Vec<String> = if cfg!(windows) {
        env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|dir| {
            extensions
                .iter()
                .any(|ext| dir.join(format!("{name}{ext}")).is_file())
        })
    })
}

// ---------------------------------------------------------------- running programs

/// The cargo that started this program, so the same toolchain is used throughout.
fn cargo() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Runs a program with its output shown, and fails if it fails.
fn run(program: &str, args: &[&str]) -> Result {
    let name = short_name(program);
    println!(
        "{}",
        paint(Color::Dim, &format!("> {name} {}", args.join(" ")))
    );
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("Could not start {name}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} failed ({status})"))
    }
}

/// "cargo" for "/home/me/.rustup/toolchains/stable-.../bin/cargo", so messages stay short.
fn short_name(program: &str) -> String {
    Path::new(program)
        .file_stem()
        .map_or_else(|| program.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Runs a program with its output shown and reports whether it succeeded.
fn shows_ok(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

/// Runs a program silently and reports whether it succeeded.
fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Runs a program silently and returns its output, if it succeeded.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------- printing

#[derive(Clone, Copy)]
enum Color {
    Red,
    Yellow,
    Cyan,
    Dim,
}

/// Colors text on terminals that understand it. Windows gets plain text,
/// because older Windows consoles print the color codes as characters.
fn paint(color: Color, text: &str) -> String {
    if cfg!(windows) || !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    let code = match color {
        Color::Red => "31",
        Color::Yellow => "33",
        Color::Cyan => "1;36",
        Color::Dim => "2",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

fn step(title: &str) {
    println!("\n{}", paint(Color::Cyan, &format!("== {title}")));
}

fn note(text: &str) {
    println!("{}", paint(Color::Yellow, text));
}

fn print_status(ok: bool, name: &str) {
    if ok {
        println!("  ok       {name}");
    } else {
        println!("{}", paint(Color::Yellow, &format!("  missing  {name}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_section_of_the_real_dependencies_file() {
        env::set_current_dir(repo_root()).unwrap();
        assert_eq!(rust_toolchain().unwrap(), "stable");
        assert!(first_words(section("rust-components").unwrap()).contains(&"clippy".to_string()));
        assert!(first_words(section("apt").unwrap()).contains(&"pkg-config".to_string()));
        let winget = winget_entries().unwrap();
        let build_tools = winget
            .iter()
            .find(|(id, _)| id == "Microsoft.VisualStudio.2022.BuildTools")
            .expect("Build Tools is listed");
        assert!(build_tools.1.as_deref().unwrap().starts_with("--wait"));
        assert!(
            winget
                .iter()
                .all(|(id, _)| !id.contains('#') && !id.contains(' '))
        );
    }
}
