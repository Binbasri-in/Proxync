use clap::{Args, Parser, Subcommand, ValueEnum};
use proxync_core::events::{create_event_channel, ProxyncEvent};
use proxync_core::recon::scan_processes;
use proxync_core::storage::get_system_info_sync;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

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

    /// Serve a local static folder over HTTP and instantly expose it to a public tunnel
    Serve(ServeArgs),

    /// View, tail, or clear persistent CLI session logs
    Logs(LogsArgs),

    /// List supported tunnel providers (native, cloudflare, relay) and usage instructions
    #[command(alias = "provider")]
    Providers,

    /// Inspect system environment, toolchain status, and diagnostic health
    #[command(alias = "diag")]
    Doctor,

    /// Launch or locate the Proxync GUI desktop application
    Gui,

    /// Register Proxync CLI in your system terminal environment (PATH)
    #[command(alias = "install")]
    SetupPath,
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

    /// Tunnel provider to use: 'native' (default, SSH edge tunnel), 'cloudflare' (zero-config public URL), 'relay' (self-hosted ws)
    #[arg(short, long, value_enum, default_value_t = Provider::Native)]
    provider: Provider,

    /// Run in detached/quiet mode: suppress live terminal traffic while saving all logs to cli.log (like docker compose up -d)
    #[arg(short = 'd', long = "detach", visible_alias = "quiet", short_alias = 'q')]
    detach: bool,

    /// Display QR code in terminal upon tunnel launch
    #[arg(long)]
    qr: bool,

    /// Tunnel token (for relay provider)
    #[arg(long)]
    token: Option<String>,

    /// Workspace ID (for relay provider)
    #[arg(long)]
    workspace: Option<String>,

    /// Relay server WebSocket URL
    #[arg(long)]
    relay_url: Option<String>,

    /// Disable persisting session traffic to cli.log
    #[arg(long)]
    no_log: bool,
}

#[derive(Args, Debug)]
struct ProxyArgs {
    /// Target local port to intercept (e.g. 3000, 8080)
    port: u16,

    /// Run in detached/quiet mode: suppress live terminal traffic while saving all logs to cli.log (like docker compose up -d)
    #[arg(short = 'd', long = "detach", visible_alias = "quiet", short_alias = 'q')]
    detach: bool,

    /// Disable persisting session traffic to cli.log
    #[arg(long)]
    no_log: bool,
}

#[derive(Args, Debug)]
struct ServeArgs {
    /// Directory path to serve (default: current directory ".")
    #[arg(default_value = ".")]
    path: String,

    /// Local port to bind static server (0 = auto-assign ephemeral port)
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Tunnel provider to use: 'native' (default, SSH edge tunnel), 'cloudflare' (zero-config public URL), 'relay' (self-hosted ws)
    #[arg(short, long, value_enum, default_value_t = Provider::Native)]
    provider: Provider,

    /// Run in detached/quiet mode: suppress live terminal traffic while saving all logs to cli.log
    #[arg(short = 'd', long = "detach", visible_alias = "quiet", short_alias = 'q')]
    detach: bool,

    /// Display QR code in terminal upon tunnel launch
    #[arg(long)]
    qr: bool,

    /// Disable persisting session traffic to cli.log
    #[arg(long)]
    no_log: bool,
}

#[derive(Args, Debug)]
struct LogsArgs {
    /// Number of lines to display (default: 50)
    #[arg(short = 'n', long, default_value_t = 50)]
    lines: usize,

    /// Stream/follow live log updates in real-time (like tail -f)
    #[arg(short = 'f', long = "follow", visible_alias = "tail")]
    follow: bool,

    /// Clear all CLI session logs
    #[arg(long)]
    clear: bool,
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
        Some(Commands::Serve(args)) => handle_serve(args).await?,
        Some(Commands::Logs(args)) => handle_logs(args).await?,
        Some(Commands::Providers) => handle_providers()?,
        Some(Commands::Doctor) => handle_doctor()?,
        Some(Commands::Gui) => launch_gui(),
        Some(Commands::SetupPath) => handle_setup_path()?,
        None => {
            // Default interactive behavior: show quick scan if no args provided
            println!("\x1b[1;36mProxync\x1b[0m — Developer Tunneling & Reconnaissance CLI");
            println!("Run \x1b[1mproxync --help\x1b[0m for available commands.\n");
            handle_scan(ScanArgs { json: false, force: false }).await?;
        }
    }

    Ok(())
}

fn handle_setup_path() -> Result<(), Box<dyn std::error::Error>> {
    println!("\x1b[1;36m==> Configuring Proxync CLI in system environment (PATH)...\x1b[0m");
    let msg = proxync_core::cli_installer::install_cli_to_path()
        .map_err(|e| format!("Failed to configure PATH: {}", e))?;
    println!("\x1b[1;32m[OK] {}\x1b[0m", msg);
    println!("\nRestart your terminal to use \x1b[1mproxync\x1b[0m from any directory!");
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

    // Bind local intercepting proxy so that all traffic over the public tunnel
    // is captured, measured in live stats, and streamed in real-time.
    let proxy_port = match proxync_core::proxy::start_proxy(event_tx.clone(), args.port).await {
        Ok(p) => p,
        Err(_) => args.port,
    };

    let public_url = match args.provider {
        Provider::Cloudflare => {
            proxync_core::tunnel::open_cloudflare_tunnel(event_tx.clone(), tunnel_id.clone(), proxy_port).await
                .map_err(|e| format!("Failed to open Cloudflare tunnel: {}", e))?
        }
        Provider::Native => {
            proxync_core::tunnel::open_native_tunnel(event_tx.clone(), tunnel_id.clone(), proxy_port, "auto".to_string()).await
                .map_err(|e| format!("Failed to open Native tunnel: {}", e))?
        }
        Provider::Relay => {
            let token = args.token.unwrap_or_else(|| "cli-token".to_string());
            let workspace = args.workspace.unwrap_or_else(|| "default".to_string());
            proxync_core::tunnel::open_tunnel(event_tx.clone(), tunnel_id.clone(), proxy_port, token, workspace, args.relay_url).await
                .map_err(|e| format!("Failed to open Relay tunnel: {}", e))?;
            format!("ws://relay/{}", tunnel_id)
        }
    };

    println!("\x1b[1;32m✓ Tunnel Established!\x1b[0m");
    println!("  Local Target : \x1b[1mhttp://localhost:{}\x1b[0m", args.port);
    println!("  Public URL   : \x1b[1;34;4m{}\x1b[0m", public_url);

    if copy_to_clipboard(&public_url) {
        println!("  📋 \x1b[32mURL copied to clipboard!\x1b[0m");
    }

    if args.qr {
        if let Some(qr) = format_qr_code(&public_url) {
            println!("\n\x1b[1;36m==> Mobile QR Code for {}\x1b[0m\n{}", public_url, qr);
        }
    }

    let logging_enabled = !args.no_log;
    let show_terminal_arc = Arc::new(AtomicBool::new(!args.detach));
    let show_terminal_event = show_terminal_arc.clone();

    if logging_enabled {
        let start_msg = format!(
            "[{}] [INFO] [TUNNEL] Started {:?} tunnel for port {} -> {}",
            proxync_core::storage::get_current_iso_timestamp(),
            args.provider,
            args.port,
            public_url
        );
        let _ = proxync_core::storage::append_log_entry("cli".into(), start_msg).await;
    }

    if args.detach {
        let logs_file = proxync_core::storage::get_logs_dir().join("cli.log");
        println!("\n\x1b[1;33m[DETACHED MODE]\x1b[0m Live terminal traffic is paused.");
        println!("All session traffic is saving to: \x1b[1m{}\x1b[0m", logs_file.display());
        print_hotkey_bar(false);
    } else {
        println!("\nStreaming live HTTP traffic (Press \x1b[1mCtrl+C\x1b[0m to quit).");
        print_hotkey_bar(true);
    }

    let stats_arc = Arc::new(Mutex::new(SessionStats::default()));
    let stats_arc_event = stats_arc.clone();
    let started_at = std::time::Instant::now();

    // Event listener task for live request logging
    let event_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                ProxyncEvent::RequestLog { method, path, timestamp: _, .. } => {
                    let mut st = stats_arc_event.lock().await;
                    st.total_requests += 1;
                    drop(st);

                    if show_terminal_event.load(Ordering::Relaxed) {
                        println!("\x1b[1;33m--> {:<6}\x1b[0m {}", method, path);
                    }
                    if logging_enabled {
                        let line = format!(
                            "[{}] [REQ] {:<6} {}",
                            proxync_core::storage::get_current_iso_timestamp(),
                            method,
                            path
                        );
                        let _ = proxync_core::storage::append_log_entry("cli".into(), line).await;
                    }
                }
                ProxyncEvent::ResponseLog { status, duration_ms, .. } => {
                    let mut st = stats_arc_event.lock().await;
                    if status >= 200 && status < 300 {
                        st.success_2xx += 1;
                    } else if status >= 300 && status < 400 {
                        st.redirect_3xx += 1;
                    } else if status >= 400 && status < 500 {
                        st.client_err_4xx += 1;
                    } else if status >= 500 {
                        st.server_err_5xx += 1;
                    }
                    st.total_duration_ms += duration_ms;
                    drop(st);

                    if show_terminal_event.load(Ordering::Relaxed) {
                        let status_color = if status < 400 { "\x1b[1;32m" } else { "\x1b[1;31m" };
                        println!("    \x1b[90m<--\x1b[0m {}{}\x1b[0m \x1b[90m({}ms)\x1b[0m", status_color, status, duration_ms);
                    }
                    if logging_enabled {
                        let line = format!(
                            "[{}] [RES] status={} ({}ms)",
                            proxync_core::storage::get_current_iso_timestamp(),
                            status,
                            duration_ms
                        );
                        let _ = proxync_core::storage::append_log_entry("cli".into(), line).await;
                    }
                }
                ProxyncEvent::TunnelAutoClosed { tunnel_id: closed_id } => {
                    if closed_id == tunnel_id_clean {
                        println!("\x1b[31m[!] Tunnel process terminated.\x1b[0m");
                        if logging_enabled {
                            let line = format!(
                                "[{}] [WARN] [TUNNEL] Tunnel terminated (id: {})",
                                proxync_core::storage::get_current_iso_timestamp(),
                                closed_id
                            );
                            let _ = proxync_core::storage::append_log_entry("cli".into(), line).await;
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    });

    let (hotkey_exit_tx, mut hotkey_exit_rx) = tokio::sync::mpsc::channel::<()>(1);
    let hotkey_url = public_url.clone();
    let stats_for_hotkey = stats_arc.clone();
    let show_terminal_hotkey = show_terminal_arc.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            if crossterm::event::poll(std::time::Duration::from_millis(150)).unwrap_or(false) {
                if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        match key.code {
                            crossterm::event::KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                let _ = hotkey_exit_tx.send(());
                                break;
                            }
                            crossterm::event::KeyCode::Esc => {
                                let _ = hotkey_exit_tx.send(());
                                break;
                            }
                            crossterm::event::KeyCode::Char('c') | crossterm::event::KeyCode::Char('C') => {
                                if copy_to_clipboard(&hotkey_url) {
                                    println!("\x1b[1;32m📋 Copied URL to clipboard: {}\x1b[0m", hotkey_url);
                                }
                                print_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Char('Q') => {
                                if let Some(qr) = format_qr_code(&hotkey_url) {
                                    println!("\n\x1b[1;36m==> Mobile QR Code for {}\x1b[0m\n{}", hotkey_url, qr);
                                }
                                print_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            crossterm::event::KeyCode::Char('s') | crossterm::event::KeyCode::Char('S') => {
                                print_stats(&stats_for_hotkey, started_at);
                                print_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            crossterm::event::KeyCode::Char('t') | crossterm::event::KeyCode::Char('T') => {
                                let cur = show_terminal_hotkey.load(Ordering::Relaxed);
                                let next = !cur;
                                show_terminal_hotkey.store(next, Ordering::Relaxed);
                                if next {
                                    println!("\x1b[1;32m🔊 Live traffic streaming resumed.\x1b[0m");
                                } else {
                                    println!("\x1b[1;33m🔇 Live traffic streaming paused (requests still logged to cli.log).\x1b[0m");
                                }
                                print_hotkey_bar(next);
                            }
                            crossterm::event::KeyCode::Char('l') | crossterm::event::KeyCode::Char('L') => {
                                print!("\x1b[2J\x1b[1;1H");
                                println!("\x1b[1;32m✓ Tunnel Active: \x1b[1;34;4m{}\x1b[0m", hotkey_url);
                                print_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            crossterm::event::KeyCode::Char('h') | crossterm::event::KeyCode::Char('H') | crossterm::event::KeyCode::Char('?') => {
                                print_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n\x1b[33m==> Closing tunnel...\x1b[0m");
        }
        _ = hotkey_exit_rx.recv() => {
            println!("\n\x1b[33m==> Closing tunnel...\x1b[0m");
        }
        _ = event_task => {}
    }

    let _ = proxync_core::tunnel::close_tunnel(tunnel_id, Some(args.port)).await;
    let _ = proxync_core::proxy::stop_proxy(Some(args.port)).await;
    if logging_enabled {
        let close_msg = format!(
            "[{}] [INFO] [TUNNEL] Closed tunnel for port {}",
            proxync_core::storage::get_current_iso_timestamp(),
            args.port
        );
        let _ = proxync_core::storage::append_log_entry("cli".into(), close_msg).await;
    }
    println!("\x1b[32m✓ Tunnel closed cleanly.\x1b[0m");

    Ok(())
}

async fn handle_proxy(args: ProxyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let (event_tx, mut event_rx) = create_event_channel();
    let logging_enabled = !args.no_log;
    let show_terminal_arc = Arc::new(AtomicBool::new(!args.detach));
    let show_terminal_event = show_terminal_arc.clone();

    println!("\x1b[1;36m==> Starting intercepting proxy for port {}...\x1b[0m", args.port);
    let proxy_port = proxync_core::proxy::start_proxy(event_tx, args.port).await
        .map_err(|e| format!("Failed to start proxy: {}", e))?;

    println!("\x1b[1;32m✓ Proxy Listening on port {}\x1b[0m", proxy_port);
    println!("  Target Service: \x1b[1mhttp://localhost:{}\x1b[0m", args.port);
    println!("  Traffic Stream: \x1b[1;34mhttp://localhost:{}\x1b[0m (Route requests here to intercept)", proxy_port);
    if logging_enabled {
        let start_msg = format!(
            "[{}] [INFO] [PROXY] Started intercepting proxy on port {} forwarding to localhost:{}",
            proxync_core::storage::get_current_iso_timestamp(),
            proxy_port,
            args.port
        );
        let _ = proxync_core::storage::append_log_entry("cli".into(), start_msg).await;
    }

    if args.detach {
        let logs_file = proxync_core::storage::get_logs_dir().join("cli.log");
        println!("\n\x1b[1;33m[DETACHED MODE]\x1b[0m Live terminal traffic is paused.");
        println!("All session traffic is saving to: \x1b[1m{}\x1b[0m", logs_file.display());
        print_proxy_hotkey_bar(false);
    } else {
        println!("\nStreaming live HTTP traffic (Press \x1b[1mCtrl+C\x1b[0m to quit).");
        print_proxy_hotkey_bar(true);
    }

    let event_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                ProxyncEvent::RequestLog { method, path, headers, body_preview, .. } => {
                    let has_body = !body_preview.is_empty();
                    if show_terminal_event.load(Ordering::Relaxed) {
                        println!(
                            "\x1b[1;33m--> {:<6}\x1b[0m {} \x1b[90m(headers: {}, body: {} bytes)\x1b[0m",
                            method,
                            path,
                            headers.as_object().map(|o| o.len()).unwrap_or(0),
                            if has_body { body_preview.len() } else { 0 }
                        );
                    }
                    if logging_enabled {
                        let line = format!(
                            "[{}] [REQ] {:<6} {}",
                            proxync_core::storage::get_current_iso_timestamp(),
                            method,
                            path
                        );
                        let _ = proxync_core::storage::append_log_entry("cli".into(), line).await;
                    }
                }
                ProxyncEvent::ResponseLog { status, duration_ms, .. } => {
                    if show_terminal_event.load(Ordering::Relaxed) {
                        let status_color = if status < 400 { "\x1b[1;32m" } else { "\x1b[1;31m" };
                        println!("    \x1b[90m<--\x1b[0m {}{}\x1b[0m \x1b[90m({}ms)\x1b[0m", status_color, status, duration_ms);
                    }
                    if logging_enabled {
                        let line = format!(
                            "[{}] [RES] status={} ({}ms)",
                            proxync_core::storage::get_current_iso_timestamp(),
                            status,
                            duration_ms
                        );
                        let _ = proxync_core::storage::append_log_entry("cli".into(), line).await;
                    }
                }
                ProxyncEvent::TunnelStatusChanged { port, status } => {
                    println!("\x1b[36m[STATUS] Port {} is now {}\x1b[0m", port, status);
                }
                _ => {}
            }
        }
    });

    let (hotkey_exit_tx, mut hotkey_exit_rx) = tokio::sync::mpsc::channel::<()>(1);
    let show_terminal_hotkey = show_terminal_arc.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            if crossterm::event::poll(std::time::Duration::from_millis(150)).unwrap_or(false) {
                if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        match key.code {
                            crossterm::event::KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                let _ = hotkey_exit_tx.send(());
                                break;
                            }
                            crossterm::event::KeyCode::Esc => {
                                let _ = hotkey_exit_tx.send(());
                                break;
                            }
                            crossterm::event::KeyCode::Char('t') | crossterm::event::KeyCode::Char('T') => {
                                let cur = show_terminal_hotkey.load(Ordering::Relaxed);
                                let next = !cur;
                                show_terminal_hotkey.store(next, Ordering::Relaxed);
                                if next {
                                    println!("\x1b[1;32m🔊 Live traffic streaming resumed.\x1b[0m");
                                } else {
                                    println!("\x1b[1;33m🔇 Live traffic streaming paused (requests still logged to cli.log).\x1b[0m");
                                }
                                print_proxy_hotkey_bar(next);
                            }
                            crossterm::event::KeyCode::Char('l') | crossterm::event::KeyCode::Char('L') => {
                                print!("\x1b[2J\x1b[1;1H");
                                println!("\x1b[1;32m✓ Proxy Listening on port {}\x1b[0m", proxy_port);
                                print_proxy_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            crossterm::event::KeyCode::Char('h') | crossterm::event::KeyCode::Char('H') | crossterm::event::KeyCode::Char('?') => {
                                print_proxy_hotkey_bar(show_terminal_hotkey.load(Ordering::Relaxed));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n\x1b[33m==> Stopping proxy...\x1b[0m");
        }
        _ = hotkey_exit_rx.recv() => {
            println!("\n\x1b[33m==> Stopping proxy...\x1b[0m");
        }
        _ = event_task => {}
    }

    let _ = proxync_core::proxy::stop_proxy(Some(args.port)).await;
    if logging_enabled {
        let close_msg = format!(
            "[{}] [INFO] [PROXY] Stopped proxy for port {}",
            proxync_core::storage::get_current_iso_timestamp(),
            args.port
        );
        let _ = proxync_core::storage::append_log_entry("cli".into(), close_msg).await;
    }
    println!("\x1b[32m✓ Proxy stopped cleanly.\x1b[0m");

    Ok(())
}

fn handle_providers() -> Result<(), Box<dyn std::error::Error>> {
    println!("\x1b[1;36mProxync Tunnel Providers & Architecture Guide\x1b[0m\n");
    println!("Proxync supports pluggable tunneling engines to fit different network environments:\n");

    println!("\x1b[1;32m1. Native (SSH Edge Tunnel) [Default]\x1b[0m");
    println!("   • Engine     : Direct SSH reverse port-forwarding to Proxync's high-speed edge cluster.");
    println!("   • Subdomain  : Automatic secure subdomain (px-*.proxync.dev), matching desktop app behavior.");
    println!("   • Best for   : Zero third-party accounts, persistent URLs, and fast local testing.");
    println!("   • CLI Usage  :");
    println!("     $ \x1b[1mproxync tunnel 3000\x1b[0m");
    println!("     $ \x1b[1mproxync tunnel 3000 -d\x1b[0m  (detached mode: traffic saved to cli.log)\n");

    println!("\x1b[1;34m2. Cloudflare (Quick Tunnels)\x1b[0m");
    println!("   • Engine     : Cloudflare Argo/cloudflared tunnel over Cloudflare's global edge network.");
    println!("   • Subdomain  : Dynamic *.trycloudflare.com URLs with automatic TLS certificates.");
    println!("   • Best for   : Zero sign-up, sharing with external clients, and bypassing strict enterprise firewalls.");
    println!("   • CLI Usage  :");
    println!("     $ \x1b[1mproxync tunnel 3000 --provider cloudflare\x1b[0m");
    println!("     $ \x1b[1mproxync tunnel 3000 -p cloudflare -d\x1b[0m\n");

    println!("\x1b[1;35m3. Relay (Self-Hosted WebSocket Relay)\x1b[0m");
    println!("   • Engine     : Proxync WebSocket multiplexer for self-hosted or air-gapped deployments.");
    println!("   • Subdomain  : Routed through your private relay server instance.");
    println!("   • Best for   : Enterprise private intranets and customized routing.");
    println!("   • CLI Usage  :");
    println!("     $ \x1b[1mproxync tunnel 3000 --provider relay --token <TOKEN> --workspace <ID>\x1b[0m\n");

    println!("\x1b[1mHow to Add Future Providers:\x1b[0m");
    println!("   Proxync's tunnel pipeline is designed for seamless extensibility:");
    println!("   1. \x1b[36mCore Engine\x1b[0m: Add provider lifecycle in `packages/core/src/tunnel.rs`.");
    println!("      Implement tunnel spawning and forward request events to `event_tx.send(...)`.");
    println!("   2. \x1b[36mCLI Mapping\x1b[0m: Add new variant (e.g. `Pinggy`, `Tailscale`, `Ngrok`) to `enum Provider` in `packages/cli/src/main.rs`.");
    println!("   3. \x1b[36mUnified Logging\x1b[0m: All providers automatically inherit live terminal streaming, detached mode (-d), and 5MB rotated `cli.log` persistence.");
    println!();

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

async fn handle_serve(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let raw_path = std::path::Path::new(&args.path);
    if !raw_path.exists() {
        return Err(format!("Directory '{}' does not exist.", args.path).into());
    }
    let root_path = raw_path.canonicalize()
        .map_err(|e| format!("Failed to resolve path '{}': {}", args.path, e))?;
    if !root_path.is_dir() {
        return Err(format!("Path '{}' is a file, not a directory.", root_path.display()).into());
    }

    let bind_addr = format!("127.0.0.1:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await
        .map_err(|e| format!("Failed to bind local HTTP server on {}: {}", bind_addr, e))?;
    let local_port = listener.local_addr()?.port();

    println!("\x1b[1;36m==> Serving static directory:\x1b[0m \x1b[1m{}\x1b[0m", root_path.display());
    println!("    Local Server : \x1b[1;32mhttp://127.0.0.1:{}\x1b[0m", local_port);

    let server_root = root_path.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let root = server_root.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 { return; }
                    let req_str = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req_str.lines().next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let method = parts.next().unwrap_or("GET");
                    let raw_path = parts.next().unwrap_or("/");

                    if method != "GET" && method != "HEAD" {
                        let res = "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = socket.write_all(res.as_bytes()).await;
                        return;
                    }

                    let clean_path = raw_path.split('?').next().unwrap_or("/");
                    let decoded_path = clean_path.trim_start_matches('/');
                    let mut file_path = root.join(decoded_path);

                    if file_path.is_dir() {
                        file_path = file_path.join("index.html");
                    }

                    // Prevent directory traversal attacks
                    if !file_path.starts_with(&root) {
                        let res = "HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\nConnection: close\r\n\r\nForbidden";
                        let _ = socket.write_all(res.as_bytes()).await;
                        return;
                    }

                    // SPA fallback
                    if !file_path.exists() {
                        let spa_index = root.join("index.html");
                        if spa_index.exists() {
                            file_path = spa_index;
                        }
                    }

                    if file_path.exists() && file_path.is_file() {
                        if let Ok(bytes) = tokio::fs::read(&file_path).await {
                            let mime = guess_mime_type(&file_path);
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                mime,
                                bytes.len()
                            );
                            let _ = socket.write_all(header.as_bytes()).await;
                            if method == "GET" {
                                let _ = socket.write_all(&bytes).await;
                            }
                            return;
                        }
                    }

                    let not_found = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 13\r\nConnection: close\r\n\r\n404 Not Found";
                    let _ = socket.write_all(not_found.as_bytes()).await;
                }
            });
        }
    });

    handle_tunnel(TunnelArgs {
        port: local_port,
        provider: args.provider,
        detach: args.detach,
        qr: args.qr,
        token: None,
        workspace: None,
        relay_url: None,
        no_log: args.no_log,
    }).await
}

async fn handle_logs(args: LogsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let logs_dir = proxync_core::storage::get_logs_dir();
    let cli_log_path = logs_dir.join("cli.log");

    if args.clear {
        let _ = std::fs::write(&cli_log_path, "");
        let _ = std::fs::remove_file(logs_dir.join("cli.log.old"));
        println!("\x1b[1;32m✓ CLI logs cleared successfully.\x1b[0m");
        return Ok(());
    }

    if !cli_log_path.exists() {
        println!("\x1b[33mNo CLI logs recorded yet at {}\x1b[0m", cli_log_path.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&cli_log_path)?;
    let all_lines: Vec<&str> = content.lines().collect();

    if all_lines.is_empty() {
        println!("\x1b[33mCLI log file is empty ({})\x1b[0m", cli_log_path.display());
        if !args.follow {
            return Ok(());
        }
    } else {
        let start_idx = all_lines.len().saturating_sub(args.lines);
        println!("\x1b[1;36m==> Showing last {} lines of {}\x1b[0m\n", all_lines.len() - start_idx, cli_log_path.display());
        for line in &all_lines[start_idx..] {
            print_colored_log_line(line);
        }
    }

    if args.follow {
        println!("\n\x1b[90mStreaming live updates (Press Ctrl+C to quit)...\x1b[0m\n");
        let mut last_size = std::fs::metadata(&cli_log_path).map(|m| m.len()).unwrap_or(0);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if let Ok(meta) = std::fs::metadata(&cli_log_path) {
                let current_size = meta.len();
                if current_size > last_size {
                    use std::io::{Seek, SeekFrom, Read};
                    if let Ok(mut file) = std::fs::File::open(&cli_log_path) {
                        let _ = file.seek(SeekFrom::Start(last_size));
                        let mut new_content = String::new();
                        let _ = file.read_to_string(&mut new_content);
                        for line in new_content.lines() {
                            print_colored_log_line(line);
                        }
                    }
                    last_size = current_size;
                } else if current_size < last_size {
                    last_size = 0;
                }
            }
        }
    }

    Ok(())
}

fn print_colored_log_line(line: &str) {
    if line.contains("[REQ]") {
        println!("\x1b[1;33m{}\x1b[0m", line);
    } else if line.contains("[RES]") {
        if line.contains("status=2") || line.contains("status=3") {
            println!("\x1b[32m{}\x1b[0m", line);
        } else {
            println!("\x1b[31m{}\x1b[0m", line);
        }
    } else if line.contains("[WARN]") {
        println!("\x1b[33m{}\x1b[0m", line);
    } else if line.contains("[INFO]") {
        println!("\x1b[36m{}\x1b[0m", line);
    } else {
        println!("{}", line);
    }
}

fn format_qr_code(url: &str) -> Option<String> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let rendered = code
        .render::<qrcode::render::unicode::Dense1x2>()
        .quiet_zone(true)
        .build();
    Some(rendered)
}

fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::io::Write;
        let mut cmd = std::process::Command::new("clip");
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x08000000);
        if let Ok(mut child) = cmd.stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        use std::io::Write;
        for tool in &["wl-copy", "xclip"] {
            let mut cmd = std::process::Command::new(tool);
            if *tool == "xclip" {
                cmd.arg("-selection").arg("clipboard");
            }
            cmd.stdin(std::process::Stdio::piped());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Default, Clone)]
struct SessionStats {
    total_requests: u64,
    success_2xx: u64,
    redirect_3xx: u64,
    client_err_4xx: u64,
    server_err_5xx: u64,
    total_duration_ms: u64,
}

fn print_stats(stats: &Arc<Mutex<SessionStats>>, started_at: std::time::Instant) {
    let s = {
        if let Ok(guard) = stats.try_lock() {
            guard.clone()
        } else {
            return;
        }
    };
    let uptime = started_at.elapsed();
    let uptime_str = format!("{}m {}s", uptime.as_secs() / 60, uptime.as_secs() % 60);
    let avg_latency = if s.total_requests > 0 {
        s.total_duration_ms / s.total_requests
    } else {
        0
    };

    println!("\n\x1b[1;36m┌── Live Tunnel Metrics ──────────────────────────┐\x1b[0m");
    println!("\x1b[1;36m│\x1b[0m Uptime          : \x1b[1m{:<30}\x1b[0m\x1b[1;36m│\x1b[0m", uptime_str);
    println!("\x1b[1;36m│\x1b[0m Total Requests  : \x1b[1m{:<30}\x1b[0m\x1b[1;36m│\x1b[0m", s.total_requests);
    println!("\x1b[1;36m│\x1b[0m Average Latency : \x1b[1m{}ms{:<28}\x1b[0m\x1b[1;36m│\x1b[0m", avg_latency, "");
    println!("\x1b[1;36m│\x1b[0m Success (2xx)   : \x1b[32m{:<30}\x1b[0m\x1b[1;36m│\x1b[0m", s.success_2xx);
    println!("\x1b[1;36m│\x1b[0m Redirect (3xx)  : \x1b[34m{:<30}\x1b[0m\x1b[1;36m│\x1b[0m", s.redirect_3xx);
    println!("\x1b[1;36m│\x1b[0m Client Err (4xx): \x1b[33m{:<30}\x1b[0m\x1b[1;36m│\x1b[0m", s.client_err_4xx);
    println!("\x1b[1;36m│\x1b[0m Server Err (5xx): \x1b[31m{:<30}\x1b[0m\x1b[1;36m│\x1b[0m", s.server_err_5xx);
    println!("\x1b[1;36m└─────────────────────────────────────────────────┘\x1b[0m\n");
}

fn print_hotkey_bar(show_traffic: bool) {
    let traffic_badge = if show_traffic {
        "\x1b[1;32mON\x1b[0m"
    } else {
        "\x1b[1;33mPAUSED\x1b[0m"
    };
    println!(
        "\x1b[90m[Hotkeys] c: Copy URL | q: QR Code | s: Live Stats | t: Traffic ({}) | l: Clear | Esc/Ctrl+C: Quit\x1b[0m\n",
        traffic_badge
    );
}

fn print_proxy_hotkey_bar(show_traffic: bool) {
    let traffic_badge = if show_traffic {
        "\x1b[1;32mON\x1b[0m"
    } else {
        "\x1b[1;33mPAUSED\x1b[0m"
    };
    println!(
        "\x1b[90m[Hotkeys] t: Traffic ({}) | l: Clear Screen | Esc / Ctrl+C: Quit\x1b[0m\n",
        traffic_badge
    );
}

fn guess_mime_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
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
            }
            _ => panic!("expected tunnel command"),
        }
    }

    #[test]
    fn test_cli_parsing_tunnel() {
        let args = ["proxync", "tunnel", "3000", "--provider", "native"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Tunnel(t)) => {
                assert_eq!(t.port, 3000);
                assert_eq!(t.provider, Provider::Native);
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
    fn test_cli_parsing_no_log_flag() {
        let args = ["proxync", "tunnel", "3000", "--no-log"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Tunnel(t)) => {
                assert!(t.no_log);
            }
            _ => panic!("expected tunnel command"),
        }
    }

    #[test]
    fn test_cli_parsing_proxy_no_log() {
        let args = ["proxync", "proxy", "8080", "--no-log"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Proxy(p)) => {
                assert!(p.no_log);
                assert_eq!(p.port, 8080);
            }
            _ => panic!("expected proxy command"),
        }
    }

    #[test]
    fn test_cli_parsing_detach_flag() {
        let args_short = ["proxync", "tunnel", "3000", "-d"];
        let cli_short = Cli::try_parse_from(args_short).expect("parse cli");
        match cli_short.command {
            Some(Commands::Tunnel(t)) => assert!(t.detach),
            _ => panic!("expected tunnel command"),
        }

        let args_long = ["proxync", "tunnel", "3000", "--detach"];
        let cli_long = Cli::try_parse_from(args_long).expect("parse cli");
        match cli_long.command {
            Some(Commands::Tunnel(t)) => assert!(t.detach),
            _ => panic!("expected tunnel command"),
        }

        let args_quiet = ["proxync", "tunnel", "3000", "--quiet"];
        let cli_quiet = Cli::try_parse_from(args_quiet).expect("parse cli");
        match cli_quiet.command {
            Some(Commands::Tunnel(t)) => assert!(t.detach),
            _ => panic!("expected tunnel command"),
        }
    }

    #[test]
    fn test_cli_parsing_proxy_detach() {
        let args = ["proxync", "proxy", "8080", "-d"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Proxy(p)) => {
                assert!(p.detach);
                assert_eq!(p.port, 8080);
            }
            _ => panic!("expected proxy command"),
        }
    }

    #[test]
    fn test_cli_parsing_providers_command() {
        let args = ["proxync", "providers"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        assert!(matches!(cli.command, Some(Commands::Providers)));

        let alias_args = ["proxync", "provider"];
        let cli_alias = Cli::try_parse_from(alias_args).expect("parse cli");
        assert!(matches!(cli_alias.command, Some(Commands::Providers)));
    }

    #[test]
    fn test_format_qr_code() {
        let url = "https://px-test.proxync.dev";
        let qr = format_qr_code(url);
        assert!(qr.is_some());
        let qr_str = qr.unwrap();
        assert!(!qr_str.is_empty());
        let lines: Vec<&str> = qr_str.lines().collect();
        println!("QR lines: {}, columns: {}", lines.len(), lines.first().map(|l| l.chars().count()).unwrap_or(0));
        println!("{}", qr_str);
        assert!(lines.len() <= 20); // half-block dense mode is under 20 lines
    }

    #[test]
    fn test_guess_mime_type() {
        assert_eq!(guess_mime_type(std::path::Path::new("index.html")), "text/html; charset=utf-8");
        assert_eq!(guess_mime_type(std::path::Path::new("app.js")), "application/javascript; charset=utf-8");
        assert_eq!(guess_mime_type(std::path::Path::new("style.css")), "text/css; charset=utf-8");
        assert_eq!(guess_mime_type(std::path::Path::new("data.json")), "application/json");
        assert_eq!(guess_mime_type(std::path::Path::new("logo.png")), "image/png");
    }

    #[test]
    fn test_cli_parsing_tunnel_qr() {
        let args = ["proxync", "tunnel", "3000", "--qr"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Tunnel(t)) => assert!(t.qr),
            _ => panic!("expected tunnel command"),
        }
    }

    #[test]
    fn test_cli_parsing_serve() {
        let args = ["proxync", "serve", "./dist", "--port", "5000", "--qr", "-d"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Serve(s)) => {
                assert_eq!(s.path, "./dist");
                assert_eq!(s.port, 5000);
                assert!(s.qr);
                assert!(s.detach);
            }
            _ => panic!("expected serve command"),
        }
    }

    #[test]
    fn test_cli_parsing_logs() {
        let args = ["proxync", "logs", "-n", "100", "-f"];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        match cli.command {
            Some(Commands::Logs(l)) => {
                assert_eq!(l.lines, 100);
                assert!(l.follow);
                assert!(!l.clear);
            }
            _ => panic!("expected logs command"),
        }

        let clear_args = ["proxync", "logs", "--clear"];
        let cli_clear = Cli::try_parse_from(clear_args).expect("parse cli");
        match cli_clear.command {
            Some(Commands::Logs(l)) => assert!(l.clear),
            _ => panic!("expected logs command"),
        }
    }
}

