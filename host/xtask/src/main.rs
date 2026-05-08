//! cargo xtask — dev-automation CLI for iExtend.
//!
//! Run with:
//!   cargo xtask <subcommand> [args]
//!
//! Subcommands:
//!   build-windows-driver   Build iexdd.sys via MSBuild (Windows only).
//!   install-windows-driver Install the driver via pnputil + devcon (Windows, admin).
//!   sign-driver-test       Test-sign iexdd.sys with the "iExtend Dev" cert (Windows, admin).
//!   trace                  Start a WPP trace session and tail it (Windows, admin).
//!
//! On Linux, the driver subcommands print a clear "Windows only" message and exit 0
//! so CI pipelines that run `cargo xtask --help` on all platforms don't fail.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "xtask", about = "iExtend dev-automation CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build iexdd.sys via MSBuild (Windows only).
    BuildWindowsDriver {
        /// Build configuration: debug (default) or release.
        #[arg(long, default_value = "debug")]
        config: BuildConfig,

        /// Test-sign the output with the "iExtend Dev" self-signed cert.
        #[arg(long, default_value_t = false)]
        test_sign: bool,
    },

    /// Install the driver via pnputil + devcon (Windows, requires admin).
    InstallWindowsDriver {
        /// Build configuration to install from.
        #[arg(long, default_value = "debug")]
        config: BuildConfig,

        /// Fully remove the old driver before installing (useful when iterating
        /// on the INF; triggers a "driver update" path in Windows).
        #[arg(long, default_value_t = false)]
        reload: bool,
    },

    /// Test-sign iexdd.sys with the "iExtend Dev" cert (Windows, admin).
    SignDriverTest {
        /// Build configuration to sign.
        #[arg(long, default_value = "debug")]
        config: BuildConfig,

        /// Name of the certificate in PrivateCertStore.
        #[arg(long, default_value = "iExtend Dev")]
        cert_name: String,

        /// Timestamp authority URL.
        #[arg(long, default_value = "http://timestamp.digicert.com")]
        timestamp: String,
    },

    /// Start a WPP trace session for the running iexdd driver (Windows, admin).
    Trace {
        /// Path to write the ETL file.
        #[arg(long, default_value = "iexdd.etl")]
        etl: PathBuf,

        /// Stop an existing trace session before starting a new one.
        #[arg(long, default_value_t = false)]
        restart: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
enum BuildConfig {
    Debug,
    Release,
}

impl BuildConfig {
    fn as_str(self) -> &'static str {
        match self {
            BuildConfig::Debug   => "Debug",
            BuildConfig::Release => "Release",
        }
    }
}

impl std::fmt::Display for BuildConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace_root = workspace_root();

    match cli.command {
        Cmd::BuildWindowsDriver { config, test_sign } => {
            build_windows_driver(&workspace_root, config, test_sign)
        }
        Cmd::InstallWindowsDriver { config, reload } => {
            install_windows_driver(&workspace_root, config, reload)
        }
        Cmd::SignDriverTest { config, cert_name, timestamp } => {
            sign_driver_test(&workspace_root, config, &cert_name, &timestamp)
        }
        Cmd::Trace { etl, restart } => {
            trace_driver(&etl, restart)
        }
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR`.
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at xtask/; parent is host/
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("xtask must be a subdirectory of the workspace root")
        .to_owned()
}

/// Path to the iexdd driver project relative to the workspace root.
#[allow(dead_code)]
fn driver_dir(workspace: &Path) -> PathBuf {
    workspace.join("drivers").join("windows").join("iexdd")
}

/// Expected output directory for the compiled driver.
#[allow(dead_code)]
fn driver_out_dir(workspace: &Path, config: BuildConfig) -> PathBuf {
    driver_dir(workspace)
        .join("x64")
        .join(config.as_str())
        .join("iexdd")
}

// ---------------------------------------------------------------------------
// build-windows-driver
// ---------------------------------------------------------------------------

#[allow(unused_variables, unused_mut, dead_code)]
fn build_windows_driver(workspace: &Path, config: BuildConfig, test_sign: bool) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("[xtask] build-windows-driver: not running on Windows — skipping MSBuild.");
        eprintln!("[xtask] Run this command on Windows with the WDK installed.");
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let project = driver_dir(workspace).join("iexdd.vcxproj");
        if !project.exists() {
            bail!("iexdd.vcxproj not found at {}", project.display());
        }

        let sign_mode = if test_sign { "TestSign" } else { "Off" };

        println!(
            "[xtask] Building iexdd.sys (config={}, test_sign={})...",
            config, test_sign
        );

        let status = msbuild(&[
            project.to_str().unwrap(),
            &format!("/p:Configuration={}", config.as_str()),
            "/p:Platform=x64",
            &format!("/p:SignMode={}", sign_mode),
            "/p:TestCertificate=iExtend Dev",
            "/m",        // parallel build
            "/nologo",
            "/clp:ForceNoAlign;Summary",
        ])?;

        if !status.success() {
            bail!("MSBuild failed with exit code {}", status.code().unwrap_or(-1));
        }

        let out = driver_out_dir(workspace, config);
        println!("[xtask] Build succeeded. Artifacts in: {}", out.display());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// install-windows-driver
// ---------------------------------------------------------------------------

#[allow(unused_variables, dead_code)]
fn install_windows_driver(workspace: &Path, config: BuildConfig, reload: bool) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("[xtask] install-windows-driver: not running on Windows — skipping.");
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        require_admin()?;

        let out = driver_out_dir(workspace, config);
        let inf = out.join("iexdd.inf");
        let inf_str = inf.to_str().context("INF path not UTF-8")?;

        if reload {
            println!("[xtask] Removing existing iexdd installation...");
            // Ignore errors — driver may not be installed yet.
            let _ = run_cmd("devcon", &["remove", "Root\\iExtendDisplay"]);
            let _ = run_cmd("pnputil", &["/delete-driver", inf_str, "/uninstall"]);
        }

        println!("[xtask] Adding driver package...");
        run_cmd("pnputil", &["/add-driver", inf_str, "/install"])
            .context("pnputil /add-driver")?
            .check("pnputil /add-driver")?;

        println!("[xtask] Installing device...");
        run_cmd("devcon", &["install", inf_str, "Root\\iExtendDisplay"])
            .context("devcon install")?
            .check("devcon install")?;

        println!("[xtask] Driver installed. Check ms-settings:display for 'iExtend Virtual Display'.");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// sign-driver-test
// ---------------------------------------------------------------------------

#[allow(unused_variables, dead_code)]
fn sign_driver_test(
    workspace: &Path,
    config:    BuildConfig,
    cert_name: &str,
    timestamp: &str,
) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("[xtask] sign-driver-test: not running on Windows — skipping.");
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        // Guard: refuse to sign if test signing isn't enabled.
        let bcdedit = run_cmd("bcdedit", &[])
            .context("bcdedit")?;
        let output = String::from_utf8_lossy(&bcdedit.stdout_bytes);
        if !output.to_lowercase().contains("testsigning") {
            bail!(
                "Test signing does not appear to be enabled.\n\
                 Run: cargo xtask sign-driver-test after enabling it with\n\
                 host\\drivers\\windows\\iexdd\\test-signing.ps1"
            );
        }

        let out = driver_out_dir(workspace, config);
        let sys = out.join("iexdd.sys");
        let cat = out.join("iexdd.cat");

        for artifact in [&sys, &cat] {
            if !artifact.exists() {
                bail!(
                    "Artifact not found: {}. Run `cargo xtask build-windows-driver` first.",
                    artifact.display()
                );
            }
        }

        println!("[xtask] Signing {} and {}...", sys.display(), cat.display());

        run_cmd("signtool", &[
            "sign", "/v",
            "/s", "PrivateCertStore",
            "/n", cert_name,
            "/t", timestamp,
            sys.to_str().unwrap(),
            cat.to_str().unwrap(),
        ])
        .context("signtool")?
        .check("signtool sign")?;

        println!("[xtask] Signing complete.");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// trace
// ---------------------------------------------------------------------------

#[allow(unused_variables, dead_code)]
fn trace_driver(etl: &Path, restart: bool) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("[xtask] trace: not running on Windows — skipping.");
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        // WPP GUID from Trace.h: {B2A6B7C1-3E4D-5F60-8192-A3B4C5D6E7F8}
        const WPP_GUID: &str = "{B2A6B7C1-3E4D-5F60-8192-A3B4C5D6E7F8}";
        const SESSION:  &str = "iExtendTrace";

        require_admin()?;

        if restart {
            let _ = run_cmd("tracelog", &["-stop", SESSION]);
        }

        let etl_str = etl.to_str().context("ETL path not UTF-8")?;

        run_cmd("tracelog", &[
            "-start", SESSION,
            "-guid", WPP_GUID,
            "-f",    etl_str,
            "-level", "5",       // TRACE_LEVEL_VERBOSE
            "-flag",  "0xFFFF",  // all bits
        ])
        .context("tracelog -start")?
        .check("tracelog -start")?;

        println!("[xtask] WPP trace started. Output: {}", etl.display());
        println!("[xtask] To stop and decode:");
        println!("          tracelog -stop {SESSION}");
        println!("          tracefmt {etl_str} -o iexdd.txt");
        println!("[xtask] Press Ctrl+C to stop the trace session.");

        // Wait for Ctrl+C.
        ctrlc_wait();

        let _ = run_cmd("tracelog", &["-stop", SESSION]);
        println!("[xtask] Trace stopped.");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Platform utilities
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn require_admin() -> Result<()> {
    use std::os::windows::process::CommandExt;
    // whoami /groups contains "S-1-16-12288" (high integrity) when elevated.
    let out = Command::new("whoami")
        .args(["/groups"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .context("whoami /groups")?;
    let s = String::from_utf8_lossy(&out.stdout);
    if !s.contains("S-1-16-12288") {
        bail!("This subcommand requires elevation. Re-run from an Administrator prompt.");
    }
    Ok(())
}

/// Small helper: run a command, returning the ExitStatus.
/// Used for Windows-only paths; kept in all builds to avoid dead-code warnings.
#[allow(dead_code)]
fn run_cmd(program: &str, args: &[&str]) -> Result<CmdResult> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to launch `{program}`"))?;
    Ok(CmdResult {
        status: out.status,
        stdout_bytes: out.stdout,
        stderr_bytes: out.stderr,
    })
}

#[allow(dead_code)]
struct CmdResult {
    status:       ExitStatus,
    stdout_bytes: Vec<u8>,
    stderr_bytes: Vec<u8>,
}

#[allow(dead_code)]
impl CmdResult {
    fn check(self, label: &str) -> Result<Self> {
        if !self.status.success() {
            bail!(
                "`{label}` failed with exit code {}",
                self.status.code().unwrap_or(-1)
            );
        }
        Ok(self)
    }
}

/// Run MSBuild by locating it via vswhere (Windows-only).
#[cfg(target_os = "windows")]
fn msbuild(args: &[&str]) -> Result<ExitStatus> {
    // Try vswhere first to find the correct MSBuild for MSVC 2022.
    let vswhere = r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe";
    let msbuild_path = if Path::new(vswhere).exists() {
        let out = Command::new(vswhere)
            .args(["-latest", "-requires", "Microsoft.Component.MSBuild",
                   "-find", "MSBuild\\**\\Bin\\MSBuild.exe"])
            .output()
            .context("vswhere")?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if s.is_empty() { "msbuild.exe".into() } else { s }
    } else {
        "msbuild.exe".into()
    };

    let status = Command::new(&msbuild_path)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch MSBuild at {msbuild_path}"))?;

    Ok(status)
}

/// Block until Ctrl+C is received (Unix: SIGINT; Windows: Ctrl+C handler).
#[allow(dead_code)]
fn ctrlc_wait() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let _ = ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    });
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
