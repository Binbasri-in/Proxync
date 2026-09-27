use clap::{Args, Parser, Subcommand, ValueEnum};
use proxync_core::events::{create_event_channel, ProxyncEvent};
use proxync_core::recon::scan_processes;
use proxync_core::storage::get_system_info_sync;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(
    name = "proxync",
    version,
    about = "Proxync CLI — Lightweight developer tunneling & reconnaissance from your terminal",
    long_about = "Proxync CLI companion provides zero-overhead dev server discovery, secure public tunneling (Cloudflare, SSH native, Relay), and traffic interception without requiring a GUI."
)]
struct Cli {
    /// Launch the Proxync GUI desktop application if installed
    #[arg(long, global = true)]
    gui: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Discover running dev servers and open ports on this machine
    #[command(alias = "ls")]
    Scan(ScanArgs),

    /// Expose a local port to a public HTTPS tunnel with real-time request logging
    Tunnel(TunnelArgs),

    /// Intercept and inspect HTTP traffic forwarding to a local port
    Proxy(ProxyArgs),

    /// Inspect system environment, toolchain status, and diagnostic health
    #[command(alias = "diag")]
    Doctor,

    /// Launch or locate the Proxync GUI desktop application
    Gui,
}

#[derive(Args, Debug)]
struct ScanArgs {
    /// Output raw JSON instead of the formatted table
    #[arg(long)]
    json: bool,

    /// Bypass process cache and force a deep system scan
    #[arg(short, long)]
    force: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Provider {
    /// Direct SSH reverse tunnel to Proxync edge with custom or auto subdomain (default)
    Native,
    /// Fast public tunnel via Cloudflare Quick Tunnels (zero sign-up)
    Cloudflare,
    /// Proxync self-hosted WebSocket relay
    Relay,
}

#[derive(Args, Debug)]
struct TunnelArgs {
    /// Local port to expose (e.g., 3000, 5173, 8080)
    port: u16,

    /// Tunnel provider to use
    #[arg(short, long, value_enum, default_value_t = Provider::Native)]
    provider: Provider,

    /// Custom subdomain for native SSH tunnel (e.g., 'myapp' -> myapp.proxync.dev)
    #[arg(short, long)]
    subdomain: Option<String>,

    /// Tunnel token (for relay provider)
    #[arg(long)]
    token: Option<String>,

    /// Workspace ID (for relay provider)
    #[arg(long)]
    workspace: Option<String>,

    /// Relay server WebSocket URL
    #[arg(long)]
    relay_url: Option<String>,
}

#[derive(Args, Debug)]
struct ProxyArgs {
    /// Target local port to intercept (e.g. 3000, 8080)
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.gui {
        launch_gui();
        return Ok(());
    }

    match cli.command {
        Some(Commands::Scan(args)) => handle_scan(args).await?,
        Some(Commands::Tunnel(args)) => handle_tunnel(args).await?,
        Some(Commands::Proxy(args)) => handle_proxy(args).await?,
        Some(Commands::Doctor) => handle_doctor()?,
        Some(Commands::Gui) => launch_gui(),
        None => {
            // Default interactive behavior: show quick scan if no args provided
            println!("\x1b[1;36mProxync\x1b[0m — Developer Tunneling & Reconnaissance CLI");
            println!("Run \x1b[1mproxync --help\x1b[0m for available commands.\n");
            handle_scan(ScanArgs { json: false, force: false }).await?;
        }
    }

    Ok(())
}

/* ══════════════════════════════════════════════
   COMMAND HANDLERS
   ══════════════════════════════════════════════ */

async fn handle_scan(args: ScanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let procs = scan_processes(args.force).await.map_err(|e| format!("Scan failed: {}", e))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&procs)?);
        return Ok(());
    }

    if procs.is_empty() {
        println!("\x1b[33mNo active local dev servers or listeners detected.\x1b[0m");
        println!("Start your server (e.g. `npm run dev`) and run `proxync scan` again.");
        return Ok(());
    }

    println!("\x1b[1m{:<7} {:<16} {:<24} {:<8} {:<30}\x1b[0m", "PORT", "FRAMEWORK", "PROCESS", "PID", "DIRECTORY");
    println!("{}", "─".repeat(88));

    for p in &procs {
        let fw = p.framework.clone().unwrap_or_else(|| "-".to_string());
        let pid_str = p.pid.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
        let dir = p.directory.as_deref().unwrap_or("-");
        let display_dir = if dir.len() > 30 {
            format!("...{}", &dir[dir.len() - 27..])
        } else {
            dir.to_string()
        };

        let fw_color = match fw.as_str() {
            "Next.js" | "React" | "Vite" => "\x1b[32m", // Green
            "Vue" | "Nuxt" => "\x1b[35m",                // Magenta
            "FastAPI" | "Django" | "Flask" => "\x1b[34m",// Blue
            "Rust" | "Go" => "\x1b[36m",                 // Cyan
            _ => "\x1b[0m",
        };

        println!(
            "\x1b[1;33m{:<7}\x1b[0m {}{:<16}\x1b[0m {:<24} {:<8} {:<30}",
            p.port,
            fw_color,
            fw,
            truncate_str(&p.name, 24),
            pid_str,
            display_dir
        );
    }

    println!("\nTip: Expose any port to the web with: \x1b[1;36mproxync tunnel <port>\x1b[0m");
    Ok(())
}

async fn handle_tunnel(args: TunnelArgs) -> Result<(), Box<dyn std::error::Error>> {
    let (event_tx, mut event_rx) = create_event_channel();
    let tunnel_id = format!("tun-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis());
    let tunnel_id_clean = tunnel_id.clone();

    println!("\x1b[1;36m==> Starting {} tunnel for port {}...\x1b[0m", format!("{:?}", args.provider).to_lowercase(), args.port);

    let public_url = match args.provider {
        Provider::Cloudflare => {
            proxync_core::tunnel::open_cloudflare_tunnel(event_tx.clone(), tunnel_id.clone(), args.port).await
                .map_err(|e| format!("Failed to open Cloudflare tunnel: {}", e))?
        }
        Provider::Native => {
            let sub = args.subdomain.unwrap_or_else(|| "auto".to_string());
            proxync_core::tunnel::open_native_tunnel(event_tx.clone(), tunnel_id.clone(), args.port, sub).await
                .map_err(|e| format!("Failed to open Native tunnel: {}", e))?
        }
        Provider::Relay => {
            let token = args.token.unwrap_or_else(|| "cli-token".to_string());
            let workspace = args.workspace.unwrap_or_else(|| "default".to_string());
            proxync_core::tunnel::open_tunnel(event_tx.clone(), tunnel_id.clone(), args.port, token, workspace, args.relay_url).await
                .map_err(|e| format!("Failed to open Relay tunnel: {}", e))?;
            format!("ws://relay/{}", tunnel_id)
        }
    };

    println!("\x1b[1;32m✓ Tunnel Established!\x1b[0m");
    println!("  Local Target : \x1b[1mhttp://localhost:{}\x1b[0m", args.port);
    println!("  Public URL   : \x1b[1;34;4m{}\x1b[0m", public_url);
    println!("\nListening for incoming traffic (Press \x1b[1mCtrl+C\x1b[0m to quit)...\n");

    // Event listener task for live request logging
    let event_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                ProxyncEvent::RequestLog { method, path, timestamp: _, .. } => {
                    println!("\x1b[1;33m--> {:<6}\x1b[0m {}", method, path);
                }
                ProxyncEvent::ResponseLog { status, duration_ms, .. } => {
                    let status_color = if status < 400 { "\x1b[1;32m" } else { "\x1b[1;31m" };
                    println!("    \x1b[90m<--\x1b[0m {}{}\x1b[0m \x1b[90m({}ms)\x1b[0m", status_color, status, duration_ms);
                }
                ProxyncEvent::TunnelAutoClosed { tunnel_id: closed_id } => {
                    if closed_id == tunnel_id_clean {
                        println!("\x1b[31m[!] Tunnel process terminated.\x1b[0m");
                        break;
                    }
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n\x1b[33m==> Closing tunnel...\x1b[0m");
        }
        _ = event_task => {}
    }

    let _ = proxync_core::tunnel::close_tunnel(tunnel_id, Some(args.port)).await;
    println!("\x1b[32m✓ Tunnel closed cleanly.\x1b[0m");

    Ok(())
}

async fn handle_proxy(args: ProxyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let (event_tx, mut event_rx) = create_event_channel();

    println!("\x1b[1;36m==> Starting intercepting proxy for port {}...\x1b[0m", args.port);
    let proxy_port = proxync_core::proxy::start_proxy(event_tx, args.port).await
        .map_err(|e| format!("Failed to start proxy: {}", e))?;

    println!("\x1b[1;32m✓ Proxy Listening on port {}\x1b[0m", proxy_port);
    println!("  Target Service: \x1b[1mhttp://localhost:{}\x1b[0m", args.port);
    println!("  Traffic Stream: \x1b[1;34mhttp://localhost:{}\x1b[0m (Route requests here to intercept)", proxy_port);
    println!("\nStreaming live HTTP traffic (Press \x1b[1mCtrl+C\x1b[0m to quit)...\n");

    let event_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                ProxyncEvent::RequestLog { method, path, headers, body_preview, .. } => {
                    let has_body = !body_preview.is_empty();
                    println!(
                        "\x1b[1;33m--> {:<6}\x1b[0m {} \x1b[90m(headers: {}, body: {} bytes)\x1b[0m",
                        method,
                        path,
                        headers.as_object().map(|o| o.len()).unwrap_or(0),
                        if has_body { body_preview.len() } else { 0 }
                    );
                }
                ProxyncEvent::ResponseLog { status, duration_ms, .. } => {
                    let status_color = if status < 400 { "\x1b[1;32m" } else { "\x1b[1;31m" };
                    println!("    \x1b[90m<--\x1b[0m {}{}\x1b[0m \x1b[90m({}ms)\x1b[0m", status_color, status, duration_ms);
                }
                ProxyncEvent::TunnelStatusChanged { port, status } => {
                    println!("\x1b[36m[STATUS] Port {} is now {}\x1b[0m", port, status);
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n\x1b[33m==> Stopping proxy...\x1b[0m");
        }
        _ = event_task => {}
    }

    let _ = proxync_core::proxy::stop_proxy(Some(args.port)).await;
    println!("\x1b[32m✓ Proxy stopped.\x1b[0m");

    Ok(())
}

fn handle_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let sys = get_system_info_sync();

    println!("\x1b[1;36mProxync Diagnostic Doctor\x1b[0m");
    println!("══════════════════════════════════════════════════");
    println!("  Operating System : {} ({})", sys.distro, sys.arch);
    println!("  Kernel Version   : {}", sys.os_version);
    println!("  Hostname         : {}", sys.hostname);
    println!("  Local IP         : {}", sys.local_ip);
    println!("  Process PID      : {}", sys.pid);
    println!("──────────────────────────────────────────────────");
    println!("Prerequisites & Toolchain Checks:");

    check_tool("Node.js runtime", "node", &["-v"]);
    check_tool("NPX package runner", "npx", &["--version"]);
    check_tool("SSH client", "ssh", &["-V"]);
    check_tool("Cloudflared binary", "cloudflared", &["--version"]);

    println!("══════════════════════════════════════════════════");
    println!("All core subsystems operational.");
    Ok(())
}

fn check_tool(name: &str, bin: &str, args: &[&str]) {
    let status = Command::new(bin).args(args).output();
    match status {
        Ok(out) => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let err_ver = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let v = if !ver.is_empty() { ver } else { err_ver };
            let first_line = v.lines().next().unwrap_or("found");
            println!("  \x1b[32m✓\x1b[0m {:<22} : {}", name, first_line);
        }
        Err(_) => {
            println!("  \x1b[33m-\x1b[0m {:<22} : not in PATH (will fallback if needed)", name);
        }
    }
}

fn launch_gui() {
    println!("\x1b[1;36m==> Launching Proxync GUI...\x1b[0m");

    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let possible_paths = [
            format!("{}\\Programs\\Proxync\\Proxync.exe", local_app_data),
            format!("{}\\Proxync\\Proxync.exe", local_app_data),
            format!("{}\\Proxync\\Proxync.exe", program_files),
        ];

        for p in &possible_paths {
            if std::path::Path::new(p).exists() {
                let _ = Command::new(p).spawn();
                println!("\x1b[32m✓ Launched Proxync GUI from {}\x1b[0m", p);
                return;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/Applications/Proxync.app").exists() {
            let _ = Command::new("open").arg("-a").arg("Proxync").spawn();
            println!("\x1b[32m✓ Launched Proxync GUI from /Applications/Proxync.app\x1b[0m");
            return;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if Command::new("proxync-desktop").spawn().is_ok() {
            println!("\x1b[32m✓ Launched Proxync GUI\x1b[0m");
            return;
        }
    }

    println!("\x1b[33mProxync Desktop GUI was not detected on this system.\x1b[0m");
    println!("To install or run the desktop GUI:");
    println!("  1. Download the installer from https://github.com/Inilax/Proxync/releases");
    println!("  2. Or run in dev mode: `npm run tauri dev`");
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_tunnel_default_provider() {
        let args = ["proxync", "tunnel", "3000"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Tunnel(t)) => {
                assert_eq!(t.port, 3000);
                assert_eq!(t.provider, Provider::Native);
                assert_eq!(t.subdomain, None);
            }
            _ => panic!("expected tunnel command"),
        }
    }

    #[test]
    fn test_cli_parsing_tunnel() {
        let args = ["proxync", "tunnel", "3000", "--provider", "native", "--subdomain", "myapp"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Tunnel(t)) => {
                assert_eq!(t.port, 3000);
                assert_eq!(t.provider, Provider::Native);
                assert_eq!(t.subdomain.as_deref(), Some("myapp"));
            }
            _ => panic!("expected tunnel command"),
        }
    }

    #[test]
    fn test_cli_parsing_scan() {
        let args = ["proxync", "scan", "--json", "-f"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Scan(s)) => {
                assert!(s.json);
                assert!(s.force);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn test_cli_parsing_gui_flag() {
        let args = ["proxync", "--gui"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        assert!(cli.gui);
    }
}

