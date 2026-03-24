use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager, WebviewWindow};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Windows CREATE_NO_WINDOW flag to prevent console windows from flashing
/// when spawning child processes (e.g. Codex CLI, cmd.exe, node.exe).
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Clone)]
pub struct CodexProcessState {
    pub processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl Default for CodexProcessState {
    fn default() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Discover the codex binary on the system.
/// Checks: which → npm global → common paths → bare fallback.
fn find_codex_binary() -> Result<String, String> {
    // 1. Check the native installer's default location first
    //    (GUI apps often don't have ~/.local/bin in PATH)
    if let Some(home) = dirs::home_dir() {
        #[cfg(target_os = "windows")]
        let native_path = home.join(".local").join("bin").join("codex.exe");
        #[cfg(not(target_os = "windows"))]
        let native_path = home.join(".local").join("bin").join("codex");
        if native_path.exists() {
            return Ok(native_path.to_string_lossy().to_string());
        }
    }

    // 2. Try to find codex on PATH
    if let Ok(path) = which::which("codex") {
        return Ok(path.to_string_lossy().to_string());
    }

    // 3. Check NVM directories (Unix) or npm global (Windows)
    #[allow(unused_variables)]
    if let Some(home) = dirs::home_dir() {
        #[cfg(not(target_os = "windows"))]
        {
            let nvm_dir = home.join(".nvm").join("versions").join("node");
            if nvm_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
                    let mut candidates: Vec<PathBuf> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.path().join("bin").join("codex"))
                        .filter(|p| p.exists())
                        .collect();
                    // Sort by version (directory name) descending to prefer latest
                    candidates.sort();
                    candidates.reverse();
                    if let Some(path) = candidates.first() {
                        return Ok(path.to_string_lossy().to_string());
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Check common Windows Node.js locations
            if let Ok(appdata) = std::env::var("APPDATA") {
                let npm_global = PathBuf::from(&appdata).join("npm").join("codex.cmd");
                if npm_global.exists() {
                    return Ok(npm_global.to_string_lossy().to_string());
                }
            }
            // NVM for Windows
            if let Ok(nvm_home) = std::env::var("NVM_HOME") {
                // nvm symlink lives under NVM_SYMLINK (default: C:\Program Files\nodejs)
                if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
                    let p = PathBuf::from(&nvm_symlink).join("codex.cmd");
                    if p.exists() {
                        return Ok(p.to_string_lossy().to_string());
                    }
                }
                // Also scan NVM_HOME/<version>
                if let Ok(entries) = std::fs::read_dir(&nvm_home) {
                    let mut candidates: Vec<PathBuf> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.path().join("codex.cmd"))
                        .filter(|p| p.exists())
                        .collect();
                    candidates.sort();
                    candidates.reverse();
                    if let Some(path) = candidates.first() {
                        return Ok(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // 4. Check standard paths (Unix only)
    #[cfg(not(target_os = "windows"))]
    {
        let standard_paths = [
            "/usr/local/bin/codex",
            "/opt/homebrew/bin/codex",
            "/usr/bin/codex",
            "/bin/codex",
        ];
        for path in &standard_paths {
            if PathBuf::from(path).exists() {
                return Ok(path.to_string());
            }
        }
    }

    // 5. Check user-specific paths
    if let Some(home) = dirs::home_dir() {
        #[cfg(not(target_os = "windows"))]
        let user_paths = vec![
            home.join(".npm-global").join("bin").join("codex"),
            home.join(".yarn").join("bin").join("codex"),
            home.join(".bun").join("bin").join("codex"),
            home.join("bin").join("codex"),
        ];
        #[cfg(target_os = "windows")]
        let user_paths = vec![
            home.join("AppData")
                .join("Local")
                .join("Programs")
                .join("codex")
                .join("codex.exe"),
        ];

        for path in &user_paths {
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }
    }

    // 6. Bare fallback — hope it's in PATH
    Ok("codex".to_string())
}

/// Strip ANSI escape sequences from CLI output before sending to the frontend.
/// Handles CSI sequences (e.g. colors, cursor), OSC sequences, and private mode
/// sequences like `\x1b[?2026h` emitted by modern CLIs.
fn strip_ansi(s: &str) -> Cow<'_, str> {
    if !s.contains('\x1b') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                // CSI: ESC [ ... (letter)
                Some('[') => {
                    chars.next();
                    // consume until final byte (ASCII letter or ~)
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() || ch == '~' {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... (ST or BEL)
                Some(']') => {
                    chars.next();
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // Two-character sequences: ESC ( , ESC ) , etc.
                Some(&ch) if ch.is_ascii_alphabetic() || ch == '(' || ch == ')' => {
                    chars.next();
                }
                _ => {}
            }
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Strip interior nul bytes that would cause Command::spawn() to fail.
/// Returns a borrowed reference when no nul bytes are present (zero-alloc fast path).
fn strip_nul(s: &str) -> Cow<'_, str> {
    if s.contains('\0') {
        eprintln!(
            "[codex-spawn] stripped {} nul byte(s) from input",
            s.matches('\0').count()
        );
        Cow::Owned(s.replace('\0', ""))
    } else {
        Cow::Borrowed(s)
    }
}

/// Environment variables needed by child processes on Linux desktops.
#[cfg(target_os = "linux")]
const LINUX_DESKTOP_ENV_VARS: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "DESKTOP_SESSION",
];

/// Sanitize environment for a child process spawned from an AppImage.
#[cfg(target_os = "linux")]
fn sanitize_appimage_env(cmd: &mut tokio::process::Command) {
    cmd.stdin(std::process::Stdio::null());

    if std::env::var("APPIMAGE").is_ok() {
        for key in &[
            "LD_LIBRARY_PATH",
            "PATH",
            "GDK_PIXBUF_MODULE_FILE",
            "PYTHONPATH",
            "PERLLIB",
            "GSETTINGS_SCHEMA_DIR",
        ] {
            let orig_key = format!("{}_ORIG", key);
            match std::env::var(&orig_key) {
                Ok(orig) => {
                    cmd.env(key, orig);
                }
                Err(_) => {
                    cmd.env_remove(key);
                }
            }
        }
        cmd.env_remove("GDK_BACKEND");
        cmd.env_remove("GIO_MODULE_DIR");
        cmd.env_remove("GIO_EXTRA_MODULES");
    }

    for key in LINUX_DESKTOP_ENV_VARS {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

/// On Windows, resolve a `.cmd` wrapper to its underlying Node.js script
/// so we can run `node <script.js>` directly, avoiding cmd.exe escaping issues.
/// Returns (program, extra_prefix_args).
#[cfg(target_os = "windows")]
fn resolve_cmd_to_node(program: &str) -> (String, Vec<String>) {
    let lower = program.to_lowercase();
    if !lower.ends_with(".cmd") && !lower.ends_with(".bat") {
        return (program.to_string(), vec![]);
    }
    // Try to find the JS entry point next to the .cmd file
    let cmd_dir = std::path::Path::new(program)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let cli_js = cmd_dir
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("bin")
        .join("codex.js");
    if cli_js.exists() {
        let node = {
            let local_node = cmd_dir.join("node.exe");
            if local_node.exists() {
                local_node.to_string_lossy().to_string()
            } else {
                "node".to_string()
            }
        };
        return (node, vec![cli_js.to_string_lossy().to_string()]);
    }
    // Fallback: use cmd.exe /C (may have issues with special chars in args)
    (
        "cmd.exe".to_string(),
        vec!["/C".to_string(), program.to_string()],
    )
}

/// Create a std::process::Command that handles .cmd/.bat files on Windows.
fn new_sync_command(program: &str) -> std::process::Command {
    #[cfg(target_os = "windows")]
    {
        let (resolved, prefix) = resolve_cmd_to_node(program);
        let mut c = std::process::Command::new(&resolved);
        c.creation_flags(CREATE_NO_WINDOW);
        if !prefix.is_empty() {
            c.args(&prefix);
        }
        return c;
    }
    #[cfg(not(target_os = "windows"))]
    std::process::Command::new(program)
}

/// Create a tokio Command with appropriate environment variables.
/// Note: Codex uses `-C <dir>` for working directory instead of current_dir,
/// but we still set current_dir as a fallback and for PATH/venv resolution.
fn create_codex_command(
    program: &str,
    args: Vec<String>,
    cwd: &str,
) -> Command {
    let clean_program = strip_nul(program);
    let clean_args: Vec<Cow<str>> = args.iter().map(|a| strip_nul(a)).collect();
    let clean_cwd = strip_nul(cwd);

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let (resolved, prefix) = resolve_cmd_to_node(clean_program.as_ref());
        let mut c = Command::new(&resolved);
        c.creation_flags(CREATE_NO_WINDOW);
        if !prefix.is_empty() {
            c.args(&prefix);
        }
        c.args(clean_args.iter().map(|a| a.as_ref()));
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = Command::new(clean_program.as_ref());
        c.args(clean_args.iter().map(|a| a.as_ref()));
        c
    };
    cmd.current_dir(clean_cwd.as_ref());

    // Pipe stdout and stderr for streaming
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // On Linux AppImage, restore original environment so child processes work correctly
    #[cfg(target_os = "linux")]
    sanitize_appimage_env(&mut cmd);

    // Build PATH: start with current PATH, prepend program dir and venv bin
    let mut current_path = strip_nul(&std::env::var("PATH").unwrap_or_default()).into_owned();
    #[cfg(target_os = "windows")]
    let sep = ";";
    #[cfg(not(target_os = "windows"))]
    let sep = ":";

    // Add the program's parent directory to PATH if not already present
    if let Some(program_dir) = std::path::Path::new(program).parent() {
        let program_dir_str = program_dir.to_string_lossy();
        if !current_path.contains(program_dir_str.as_ref()) {
            current_path = format!("{}{}{}", program_dir_str, sep, current_path);
        }
    }

    // Auto-detect project venv and inject VIRTUAL_ENV + PATH
    let venv_dir = std::path::Path::new(cwd).join(".venv");
    if venv_dir.exists() {
        cmd.env("VIRTUAL_ENV", &venv_dir);
        #[cfg(not(target_os = "windows"))]
        let venv_bin = venv_dir.join("bin");
        #[cfg(target_os = "windows")]
        let venv_bin = venv_dir.join("Scripts");
        current_path = format!("{}{}{}", venv_bin.to_string_lossy(), sep, current_path);
    }

    cmd.env("PATH", current_path);

    cmd
}

// ─── Event payloads (include tab_id for multi-tab routing) ───

#[derive(Clone, serde::Serialize)]
struct CodexOutputEvent {
    tab_id: String,
    data: String,
}

#[derive(Clone, serde::Serialize)]
struct CodexCompleteEvent {
    tab_id: String,
    success: bool,
}

#[derive(Clone, serde::Serialize)]
struct CodexErrorEvent {
    tab_id: String,
    data: String,
}

/// Spawn the Codex CLI process and stream output via Tauri events.
/// Events are emitted only to the originating window, tagged with tab_id.
async fn spawn_codex_process(
    window: WebviewWindow,
    mut cmd: Command,
    tab_id: String,
) -> Result<(), String> {
    let window_label = window.label().to_string();
    let process_key = format!("{}:{}", window_label, tab_id);

    // Spawn the process
    let mut child = cmd.spawn().map_err(|e| {
        eprintln!(
            "[codex-spawn] Failed to spawn process for tab {}: {}",
            tab_id, e
        );
        format!(
            "Failed to spawn Codex process: {}. Is Codex CLI installed?",
            e
        )
    })?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    // Get a clone of the process state Arc before any moves
    let process_arc = window
        .state::<CodexProcessState>()
        .inner()
        .processes
        .clone();

    // Store the child process in state (kill any existing process for this tab)
    {
        let mut processes = process_arc.lock().await;
        if let Some(mut existing) = processes.remove(&process_key) {
            let _ = existing.kill().await;
        }
        processes.insert(process_key.clone(), child);
    }

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    let start_time = std::time::Instant::now();

    // Spawn stdout streaming task — emit JSONL lines to the originating window
    let win_stdout = window.clone();
    let tab_id_stdout = tab_id.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = stdout_reader.lines();
        let mut line_count: u64 = 0;
        while let Ok(Some(line)) = lines.next_line().await {
            line_count += 1;
            let elapsed = start_time.elapsed().as_secs_f64();

            // Log JSONL message type for debugging
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                eprintln!(
                    "[codex-stdout] [{}] +{:.1}s #{} type={} len={}",
                    tab_id_stdout,
                    elapsed,
                    line_count,
                    msg_type,
                    line.len()
                );
            }

            // Emit output event to this window with tab_id
            let _ = win_stdout.emit(
                "codex-output",
                CodexOutputEvent {
                    tab_id: tab_id_stdout.clone(),
                    data: line,
                },
            );
        }
        eprintln!(
            "[codex-stdout] [{}] stream ended after {} lines ({:.1}s)",
            tab_id_stdout,
            line_count,
            start_time.elapsed().as_secs_f64()
        );
    });

    // Spawn stderr streaming task — emit only to the originating window
    let win_stderr = window.clone();
    let tab_id_stderr = tab_id.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = stderr_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            eprintln!(
                "[codex-stderr] [{}] +{:.1}s {}",
                tab_id_stderr,
                start_time.elapsed().as_secs_f64(),
                &line[..line.len().min(200)]
            );
            let _ = win_stderr.emit(
                "codex-error",
                CodexErrorEvent {
                    tab_id: tab_id_stderr.clone(),
                    data: line,
                },
            );
        }
    });

    // Spawn wait task — wait for process completion
    let process_arc_wait = process_arc.clone();
    let win_wait = window;
    let process_key_wait = process_key;
    let tab_id_wait = tab_id;
    tokio::spawn(async move {
        // Wait for stdout/stderr to finish
        let _ = stdout_task.await;
        let _ = stderr_task.await;

        // Wait for process exit and remove from map
        let mut processes = process_arc_wait.lock().await;
        let success = if let Some(mut child) = processes.remove(&process_key_wait) {
            match child.wait().await {
                Ok(status) => {
                    eprintln!(
                        "[codex-process] [{}] exited with status={} ({:.1}s)",
                        tab_id_wait,
                        status,
                        start_time.elapsed().as_secs_f64()
                    );
                    status.success()
                }
                Err(e) => {
                    eprintln!(
                        "[codex-process] [{}] wait error: {} ({:.1}s)",
                        tab_id_wait,
                        e,
                        start_time.elapsed().as_secs_f64()
                    );
                    false
                }
            }
        } else {
            eprintln!(
                "[codex-process] [{}] no child found in map ({:.1}s)",
                tab_id_wait,
                start_time.elapsed().as_secs_f64()
            );
            false
        };
        drop(processes);

        // Emit completion event to this window with tab_id
        let _ = win_wait.emit(
            "codex-complete",
            CodexCompleteEvent {
                tab_id: tab_id_wait,
                success,
            },
        );
    });

    Ok(())
}

// ─── Tauri Commands ───

/// Check if Codex CLI is installed and return version string.
#[tauri::command]
pub async fn check_codex_status() -> Result<String, String> {
    let binary_path = find_codex_binary()?;

    let version_output = new_sync_command(&binary_path).arg("--version").output();

    match version_output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(format!("Codex CLI returned error: {}", stderr))
        }
        Err(e) => Err(format!(
            "Codex CLI not found or not working: {}",
            e
        )),
    }
}

/// Start a new Codex exec session with JSONL output.
/// Command: codex exec --json --dangerously-bypass-approvals-and-sandbox -C <project_dir> -m <model> "<prompt>"
#[tauri::command]
pub async fn execute_codex_code(
    window: WebviewWindow,
    project_path: String,
    prompt: String,
    tab_id: String,
    model: Option<String>,
) -> Result<(), String> {
    let codex_path = find_codex_binary()?;

    let mut args = vec![
        "exec".to_string(),
        "--json".to_string(),
        "--dangerously-bypass-approvals-and-sandbox".to_string(),
        "-C".to_string(),
        project_path.clone(),
    ];
    if let Some(m) = model {
        args.push("-m".to_string());
        args.push(m);
    }
    args.push(prompt);

    let cmd = create_codex_command(&codex_path, args, &project_path);
    spawn_codex_process(window, cmd, tab_id).await
}

/// Continue/resume the last Codex session.
/// Command: codex exec resume --json --dangerously-bypass-approvals-and-sandbox -C <project_dir> -m <model> --last "<prompt>"
#[tauri::command]
pub async fn continue_codex_code(
    window: WebviewWindow,
    project_path: String,
    prompt: String,
    tab_id: String,
    model: Option<String>,
) -> Result<(), String> {
    let codex_path = find_codex_binary()?;

    let mut args = vec![
        "exec".to_string(),
        "resume".to_string(),
        "--json".to_string(),
        "--dangerously-bypass-approvals-and-sandbox".to_string(),
        "-C".to_string(),
        project_path.clone(),
    ];
    if let Some(m) = model {
        args.push("-m".to_string());
        args.push(m);
    }
    args.push("--last".to_string());
    args.push(prompt);

    let cmd = create_codex_command(&codex_path, args, &project_path);
    spawn_codex_process(window, cmd, tab_id).await
}

/// Kill a running Codex process for a specific tab.
#[tauri::command]
pub async fn cancel_codex_execution(window: WebviewWindow, tab_id: String) -> Result<(), String> {
    let window_label = window.label().to_string();
    let process_key = format!("{}:{}", window_label, tab_id);
    let codex_state = window.state::<CodexProcessState>();
    let mut processes = codex_state.processes.lock().await;
    if let Some(mut child) = processes.remove(&process_key) {
        let _ = child.kill().await;
        let _ = window.emit(
            "codex-complete",
            CodexCompleteEvent {
                tab_id,
                success: false,
            },
        );
    }
    Ok(())
}

/// Run `codex login` to authenticate.
#[tauri::command]
pub async fn codex_login(window: WebviewWindow) -> Result<(), String> {
    let binary_path = find_codex_binary()?;

    // Verify it actually exists
    let version_check = new_sync_command(&binary_path).arg("--version").output();
    if !version_check.as_ref().is_ok_and(|o| o.status.success()) {
        return Err("Codex CLI is not properly installed".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let (resolved, prefix) = resolve_cmd_to_node(&binary_path);
        let mut c = tokio::process::Command::new(&resolved);
        c.creation_flags(CREATE_NO_WINDOW);
        if !prefix.is_empty() {
            c.args(&prefix);
        }
        c.arg("login");
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new(&binary_path);
        c.arg("login");
        c
    };
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null());

    #[cfg(target_os = "linux")]
    sanitize_appimage_env(&mut cmd);

    // Inherit essential environment variables
    for (key, value) in std::env::vars() {
        if key == "PATH"
            || key == "HOME"
            || key == "USER"
            || key == "SHELL"
            || key == "LANG"
            || key.starts_with("LC_")
            || key == "HOMEBREW_PREFIX"
            || key == "HOMEBREW_CELLAR"
            || key == "HTTP_PROXY"
            || key == "HTTPS_PROXY"
            || key == "NO_PROXY"
            || key == "ALL_PROXY"
        {
            cmd.env(&key, &value);
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run codex login: {}", e))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    // Stream stdout
    let win_stdout = window.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = stdout_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let clean = strip_ansi(&line);
            let _ = win_stdout.emit("codex-login-output", clean.as_ref());
        }
    });

    // Stream stderr
    let win_stderr = window.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = stderr_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let clean = strip_ansi(&line);
            let _ = win_stderr.emit("codex-login-error", clean.as_ref());
        }
    });

    // Wait for completion with a timeout
    let win_complete = window;
    let child = Arc::new(Mutex::new(child));
    let child_for_timeout = child.clone();
    tokio::spawn(async move {
        let timeout_duration = tokio::time::Duration::from_secs(120);
        let wait_result = tokio::time::timeout(timeout_duration, async {
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            child.lock().await.wait().await
        })
        .await;

        let success = match wait_result {
            Ok(Ok(status)) => status.success(),
            Ok(Err(_)) => false,
            Err(_) => {
                // Timeout — kill the stuck process
                let _ = child_for_timeout.lock().await.kill().await;
                false
            }
        };

        let _ = win_complete.emit("codex-login-complete", success);
    });

    Ok(())
}

/// Run `codex logout` to deauthenticate.
#[tauri::command]
pub async fn codex_logout(window: WebviewWindow) -> Result<(), String> {
    let binary_path = find_codex_binary()?;

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let (resolved, prefix) = resolve_cmd_to_node(&binary_path);
        let mut c = tokio::process::Command::new(&resolved);
        c.creation_flags(CREATE_NO_WINDOW);
        if !prefix.is_empty() {
            c.args(&prefix);
        }
        c.arg("logout");
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new(&binary_path);
        c.arg("logout");
        c
    };
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null());

    #[cfg(target_os = "linux")]
    sanitize_appimage_env(&mut cmd);

    // Inherit essential environment variables
    for (key, value) in std::env::vars() {
        if key == "PATH"
            || key == "HOME"
            || key == "USER"
            || key == "SHELL"
            || key == "LANG"
            || key.starts_with("LC_")
        {
            cmd.env(&key, &value);
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run codex logout: {}", e))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    // Stream stdout
    let win_stdout = window.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = stdout_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let clean = strip_ansi(&line);
            let _ = win_stdout.emit("codex-logout-output", clean.as_ref());
        }
    });

    // Stream stderr
    let win_stderr = window.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = stderr_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let clean = strip_ansi(&line);
            let _ = win_stderr.emit("codex-logout-error", clean.as_ref());
        }
    });

    // Wait for completion
    let win_complete = window;
    tokio::spawn(async move {
        let _ = stdout_task.await;
        let _ = stderr_task.await;

        let success = match child.wait().await {
            Ok(status) => status.success(),
            Err(_) => false,
        };

        let _ = win_complete.emit("codex-logout-complete", success);
    });

    Ok(())
}

/// Kill all Codex processes associated with a specific window label.
/// Called when a window is destroyed.
pub async fn kill_process_for_window(state: &CodexProcessState, window_label: &str) {
    let mut processes = state.processes.lock().await;
    let prefix = format!("{}:", window_label);
    let keys_to_remove: Vec<String> = processes
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    for key in keys_to_remove {
        if let Some(mut child) = processes.remove(&key) {
            let _ = child.kill().await;
        }
    }
}
