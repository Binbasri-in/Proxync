use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliStatus {
    pub is_installed: bool,
    pub binary_path: Option<String>,
    pub install_dir: String,
    pub in_path: bool,
}

/// Resolves standard OS-specific installation directory for the Proxync CLI
pub fn get_default_cli_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data).join("Programs").join("Proxync").join("bin")
        } else if let Ok(user_profile) = std::env::var("USERPROFILE") {
            PathBuf::from(user_profile).join("AppData").join("Local").join("Programs").join("Proxync").join("bin")
        } else {
            PathBuf::from(r"C:\ProgramData\Proxync\bin")
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local").join("bin")
        } else {
            PathBuf::from("/usr/local/bin")
        }
    }
}

fn is_same_path(path_a: &Path, path_b: &Path) -> bool {
    if let (Ok(can_a), Ok(can_b)) = (path_a.canonicalize(), path_b.canonicalize()) {
        can_a == can_b
    } else {
        path_a == path_b
    }
}

fn is_desktop_binary(path: &Path, current_exe: Option<&Path>) -> bool {
    if let Some(curr) = current_exe {
        if is_same_path(path, curr) {
            return true;
        }
    }
    let s = path.to_string_lossy().to_lowercase();
    s.contains("src-tauri") || s.contains("packages\\desktop") || s.contains("packages/desktop")
}

/// Checks if the Proxync CLI binary is installed and whether it is discoverable in PATH
pub fn check_cli_status() -> CliStatus {
    let install_dir = get_default_cli_dir();
    let bin_name = if cfg!(target_os = "windows") { "proxync.exe" } else { "proxync" };
    let expected_binary = install_dir.join(bin_name);

    let current_exe = std::env::current_exe().ok();

    // 1. Check if it exists in the standard install directory
    let mut binary_path = if expected_binary.is_file() {
        Some(expected_binary.to_string_lossy().to_string())
    } else {
        None
    };

    // 2. Check if any genuine CLI binary is reachable via system PATH
    let mut in_path = false;
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(bin_name);
            if candidate.is_file() {
                // Ignore any desktop GUI binary or active GUI process
                if is_desktop_binary(&candidate, current_exe.as_deref()) {
                    continue;
                }

                in_path = true;
                if binary_path.is_none() {
                    binary_path = Some(candidate.to_string_lossy().to_string());
                }
                break;
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On Unix, also verify /usr/local/bin/proxync explicitly
        let usr_local = Path::new("/usr/local/bin").join(bin_name);
        if usr_local.is_file() {
            in_path = true;
            if binary_path.is_none() {
                binary_path = Some(usr_local.to_string_lossy().to_string());
            }
        }
    }

    let is_installed = expected_binary.is_file() || in_path;

    CliStatus {
        is_installed,
        binary_path: if is_installed { binary_path } else { None },
        install_dir: install_dir.to_string_lossy().to_string(),
        in_path,
    }
}

/// Discovers candidate source CLI binaries from app bundle, current exe folder, or dev target
pub fn find_source_cli_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(target_os = "windows") { "proxync.exe" } else { "proxync" };
    let current_exe = std::env::current_exe().ok();

    // 1. Walk up from current_dir to locate the repo/workspace root and find packages/cli/target/{release,debug}
    if let Ok(cwd) = std::env::current_dir() {
        let mut curr: Option<&Path> = Some(&cwd);
        while let Some(dir) = curr {
            let release = dir.join("packages").join("cli").join("target").join("release").join(bin_name);
            if release.is_file() && !is_desktop_binary(&release, current_exe.as_deref()) {
                return Some(release);
            }
            let debug = dir.join("packages").join("cli").join("target").join("debug").join(bin_name);
            if debug.is_file() && !is_desktop_binary(&debug, current_exe.as_deref()) {
                return Some(debug);
            }
            curr = dir.parent();
        }
    }

    // 2. Walk up from current_exe to locate packaged resources, bin, or dev targets
    if let Some(ref exe_path) = current_exe {
        let mut curr: Option<&Path> = exe_path.parent();
        while let Some(parent) = curr {
            // Check packaged app layouts:
            // Windows/Linux installed layout: <InstallDir>/bin/proxync.exe
            let sub_bin = parent.join("bin").join(bin_name);
            if sub_bin.is_file() && !is_desktop_binary(&sub_bin, Some(exe_path)) {
                return Some(sub_bin);
            }
            // macOS bundle layout: Proxync.app/Contents/Resources/bin/proxync
            let mac_res = parent.join("Resources").join("bin").join(bin_name);
            if mac_res.is_file() && !is_desktop_binary(&mac_res, Some(exe_path)) {
                return Some(mac_res);
            }
            let mac_res2 = parent.join("../Resources/bin").join(bin_name);
            if mac_res2.is_file() && !is_desktop_binary(&mac_res2, Some(exe_path)) {
                return Some(mac_res2);
            }
            // Development workspace layouts:
            let dev_cli_release = parent.join("packages").join("cli").join("target").join("release").join(bin_name);
            if dev_cli_release.is_file() && !is_desktop_binary(&dev_cli_release, Some(exe_path)) {
                return Some(dev_cli_release);
            }
            let dev_cli_debug = parent.join("packages").join("cli").join("target").join("debug").join(bin_name);
            if dev_cli_debug.is_file() && !is_desktop_binary(&dev_cli_debug, Some(exe_path)) {
                return Some(dev_cli_debug);
            }

            curr = parent.parent();
        }
    }

    None
}

/// Installs the Proxync CLI binary to the default OS directory and registers it to User PATH
pub fn install_cli_to_path() -> Result<String, String> {
    let target_dir = get_default_cli_dir();
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("Failed to create target directory: {}", e))?;

    let bin_name = if cfg!(target_os = "windows") { "proxync.exe" } else { "proxync" };
    let destination = target_dir.join(bin_name);

    if let Some(source) = find_source_cli_binary() {
        if source != destination {
            std::fs::copy(&source, &destination)
                .map_err(|e| format!("Failed to copy CLI binary to {}: {}", destination.display(), e))?;
        }
    } else if !destination.is_file() {
        return Err("Could not locate source proxync binary to install. Please build with 'npm run build:cli' first.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        // Add target_dir to Windows User PATH via PowerShell
        let dir_str = target_dir.to_string_lossy().to_string();
        let ps_cmd = format!(
            "$dir = '{}'; $p = [Environment]::GetEnvironmentVariable('Path', 'User'); \
             if ($p -notlike ('*' + $dir + '*')) {{ \
                 $newP = if ([string]::IsNullOrEmpty($p)) {{ $dir }} else {{ ($p.TrimEnd(';') + ';' + $dir) }}; \
                 [Environment]::SetEnvironmentVariable('Path', $newP, 'User'); \
             }}",
            dir_str.replace('\'', "''")
        );

        let mut cmd = std::process::Command::new("powershell");
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let status = cmd
            .args(&["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
            .status()
            .map_err(|e| format!("Failed to execute PowerShell PATH update: {}", e))?;

        if !status.success() {
            return Err("PowerShell failed to update Windows User PATH".to_string());
        }

        // Also append to current process PATH so immediate checks pass
        if let Some(curr_path) = std::env::var_os("PATH") {
            let mut paths = std::env::split_paths(&curr_path).collect::<Vec<_>>();
            if !paths.contains(&target_dir) {
                paths.push(target_dir.clone());
                if let Ok(new_var) = std::env::join_paths(paths) {
                    std::env::set_var("PATH", new_var);
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&destination) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&destination, perms);
        }

        // On Unix, attempt symlinking to /usr/local/bin if writable, else ~/.local/bin is already used
        let usr_local_bin = Path::new("/usr/local/bin");
        if usr_local_bin.exists() {
            let symlink_path = usr_local_bin.join("proxync");
            if symlink_path != destination {
                let _ = std::fs::remove_file(&symlink_path);
                // Attempt symlink; if permission denied, destination in ~/.local/bin still functions
                let _ = std::os::unix::fs::symlink(&destination, &symlink_path);
            }
        }
    }

    Ok(format!("Proxync CLI successfully installed to {}", destination.display()))
}

/// Uninstalls the Proxync CLI binary and removes it from the User PATH
pub fn uninstall_cli_from_path() -> Result<String, String> {
    let target_dir = get_default_cli_dir();
    let bin_name = if cfg!(target_os = "windows") { "proxync.exe" } else { "proxync" };
    let destination = target_dir.join(bin_name);

    if destination.exists() {
        let _ = std::fs::remove_file(&destination);
    }

    #[cfg(target_os = "windows")]
    {
        let dir_str = target_dir.to_string_lossy().to_string();
        let ps_cmd = format!(
            "$cleanDir = '{}'.TrimEnd('\\').ToLower(); \
             $p = [Environment]::GetEnvironmentVariable('Path', 'User'); \
             if ($p) {{ \
                 $parts = $p -split ';' | Where-Object {{ $_ -and $_.Trim().TrimEnd('\\').ToLower() -ne $cleanDir }}; \
                 [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User'); \
             }}",
            dir_str.replace('\'', "''")
        );

        let mut cmd = std::process::Command::new("powershell");
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let _ = cmd.args(&["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd]).status();

        // Also remove target_dir from current process in-memory PATH
        if let Some(curr_path) = std::env::var_os("PATH") {
            let paths: Vec<PathBuf> = std::env::split_paths(&curr_path)
                .filter(|p| !is_same_path(p, &target_dir) && p != &target_dir)
                .collect();
            if let Ok(new_var) = std::env::join_paths(paths) {
                std::env::set_var("PATH", new_var);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::fs::remove_file("/usr/local/bin/proxync");
        let _ = std::fs::remove_file(target_dir.join("proxync"));
    }

    Ok("Proxync CLI uninstalled from PATH".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_cli_dir_not_empty() {
        let dir = get_default_cli_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_check_cli_status_returns_valid_struct() {
        let status = check_cli_status();
        assert!(!status.install_dir.is_empty());
        if status.is_installed {
            assert!(status.binary_path.is_some());
        }
    }

    #[test]
    fn test_is_desktop_binary_filters_tauri_paths() {
        assert!(is_desktop_binary(Path::new(r"C:\repo\packages\desktop\src-tauri\target\release\proxync.exe"), None));
        assert!(is_desktop_binary(Path::new("/repo/packages/desktop/src-tauri/target/debug/proxync"), None));
        assert!(!is_desktop_binary(Path::new(r"C:\repo\packages\cli\target\release\proxync.exe"), None));
        assert!(!is_desktop_binary(Path::new("/repo/packages/cli/target/release/proxync"), None));
    }

    #[test]
    fn test_find_source_cli_binary_never_returns_gui() {
        let src = find_source_cli_binary().expect("Should locate genuine CLI binary");
        println!("Found CLI binary source: {:?}", src);
        assert!(!is_desktop_binary(&src, None));
        let s = src.to_string_lossy().to_lowercase();
        assert!(!s.contains("src-tauri"));
        assert!(s.contains("cli"));
    }
}
