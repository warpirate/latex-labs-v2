use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::Mutex;

use crate::llm::{self, LlmConfig, Message};

// --- Public types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStatus {
    pub step: String,
    pub progress: f32,
    pub log: Vec<String>,
    pub error: Option<String>,
    pub complete: bool,
}

// --- Internal state ---

#[derive(Debug, Clone)]
enum TransferStep {
    AnalyzeSource,
    AnalyzeTarget,
    DraftPlan,
    ApplyTransfer,
    CopyAssets,
    Compile,
    FixCompile,
    Finalize,
    Complete,
    Failed,
}

impl TransferStep {
    fn name(&self) -> &str {
        match self {
            Self::AnalyzeSource => "analyze_source",
            Self::AnalyzeTarget => "analyze_target",
            Self::DraftPlan => "draft_plan",
            Self::ApplyTransfer => "apply_transfer",
            Self::CopyAssets => "copy_assets",
            Self::Compile => "compile",
            Self::FixCompile => "fix_compile",
            Self::Finalize => "finalize",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    fn progress(&self) -> f32 {
        match self {
            Self::AnalyzeSource => 0.1,
            Self::AnalyzeTarget => 0.2,
            Self::DraftPlan => 0.35,
            Self::ApplyTransfer => 0.55,
            Self::CopyAssets => 0.7,
            Self::Compile => 0.8,
            Self::FixCompile => 0.85,
            Self::Finalize => 0.95,
            Self::Complete => 1.0,
            Self::Failed => 1.0,
        }
    }
}

#[derive(Debug)]
struct TransferState {
    step: TransferStep,
    log: Vec<String>,
    error: Option<String>,
    cancelled: bool,

    // Config
    source_project_dir: String,
    source_main_file: String,
    target_project_dir: String,
    llm_config: LlmConfig,

    // Intermediate data
    source_content: String,
    source_outline: Vec<OutlineEntry>,
    source_assets: SourceAssets,
    target_preamble: String,
    target_template_content: String,
    transfer_plan: String,
    compile_attempt: u32,
    max_compile_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OutlineEntry {
    level: String,
    title: String,
}

#[derive(Debug, Clone, Default)]
struct SourceAssets {
    bib_files: Vec<String>,
    images: Vec<String>,
    style_files: Vec<String>,
}

// --- Global job store ---

static JOBS: LazyLock<Mutex<HashMap<String, Arc<Mutex<TransferState>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// --- Tauri commands ---

/// Start a new transfer job.
///
/// Returns a job_id that can be used with `transfer_step` and `transfer_cancel`.
#[tauri::command]
pub async fn transfer_start(
    app: tauri::AppHandle,
    source_project_dir: String,
    source_main_file: String,
    target_project_dir: String,
    llm_config: LlmConfig,
) -> Result<String, String> {
    let job_id = uuid::Uuid::new_v4().to_string();

    // Validate directories exist
    if !Path::new(&source_project_dir).exists() {
        return Err(format!(
            "Source project directory not found: {}",
            source_project_dir
        ));
    }
    if !Path::new(&target_project_dir).exists() {
        // Create target directory
        std::fs::create_dir_all(&target_project_dir)
            .map_err(|e| format!("Failed to create target directory: {}", e))?;
    }

    let state = TransferState {
        step: TransferStep::AnalyzeSource,
        log: vec![format!("[start] Transfer job {} created.", job_id)],
        error: None,
        cancelled: false,
        source_project_dir,
        source_main_file,
        target_project_dir,
        llm_config,
        source_content: String::new(),
        source_outline: Vec::new(),
        source_assets: SourceAssets::default(),
        target_preamble: String::new(),
        target_template_content: String::new(),
        transfer_plan: String::new(),
        compile_attempt: 0,
        max_compile_attempts: 5,
    };

    let arc_state = Arc::new(Mutex::new(state));
    {
        let mut jobs = JOBS.lock().await;
        jobs.insert(job_id.clone(), arc_state);
    }

    let _ = app.emit(
        "transfer-progress",
        serde_json::json!({
            "job_id": &job_id,
            "step": "created",
            "message": "Transfer job created",
        }),
    );

    Ok(job_id)
}

/// Advance the transfer by one step.
#[tauri::command]
pub async fn transfer_step(
    app: tauri::AppHandle,
    job_id: String,
) -> Result<TransferStatus, String> {
    let arc_state = {
        let jobs = JOBS.lock().await;
        jobs.get(&job_id)
            .cloned()
            .ok_or_else(|| format!("Job not found: {}", job_id))?
    };

    let mut state = arc_state.lock().await;

    if state.cancelled {
        return Ok(TransferStatus {
            step: "cancelled".to_string(),
            progress: 0.0,
            log: state.log.clone(),
            error: Some("Job was cancelled".to_string()),
            complete: true,
        });
    }

    // Execute the current step
    let step_name = state.step.name().to_string();
    let _ = app.emit(
        "transfer-progress",
        serde_json::json!({
            "job_id": &job_id,
            "step": &step_name,
            "message": format!("Starting step: {}", step_name),
        }),
    );

    let result = match state.step {
        TransferStep::AnalyzeSource => step_analyze_source(&mut state).await,
        TransferStep::AnalyzeTarget => step_analyze_target(&mut state).await,
        TransferStep::DraftPlan => step_draft_plan(&mut state).await,
        TransferStep::ApplyTransfer => step_apply_transfer(&mut state).await,
        TransferStep::CopyAssets => step_copy_assets(&mut state).await,
        TransferStep::Compile => step_compile(&mut state).await,
        TransferStep::FixCompile => step_fix_compile(&mut state).await,
        TransferStep::Finalize => step_finalize(&mut state).await,
        TransferStep::Complete | TransferStep::Failed => {
            return Ok(build_status(&state));
        }
    };

    match result {
        Ok(()) => {
            let _ = app.emit(
                "transfer-progress",
                serde_json::json!({
                    "job_id": &job_id,
                    "step": state.step.name(),
                    "message": format!("Completed step: {}", step_name),
                }),
            );
        }
        Err(err) => {
            state.log.push(format!("[error] {}: {}", step_name, err));
            state.error = Some(err);
            state.step = TransferStep::Failed;
        }
    }

    Ok(build_status(&state))
}

/// Cancel a running transfer job.
#[tauri::command]
pub async fn transfer_cancel(job_id: String) -> Result<(), String> {
    let arc_state = {
        let jobs = JOBS.lock().await;
        jobs.get(&job_id)
            .cloned()
            .ok_or_else(|| format!("Job not found: {}", job_id))?
    };

    let mut state = arc_state.lock().await;
    state.cancelled = true;
    state.log.push("[cancel] Job cancelled by user.".to_string());

    Ok(())
}

fn build_status(state: &TransferState) -> TransferStatus {
    TransferStatus {
        step: state.step.name().to_string(),
        progress: state.step.progress(),
        log: state.log.clone(),
        error: state.error.clone(),
        complete: matches!(state.step, TransferStep::Complete | TransferStep::Failed),
    }
}

// --- Step implementations ---

async fn step_analyze_source(state: &mut TransferState) -> Result<(), String> {
    let source_dir = Path::new(&state.source_project_dir);
    let main_path = source_dir.join(&state.source_main_file);

    // Read and resolve \input/\include references
    let full_content =
        resolve_inputs(source_dir, &state.source_main_file, &mut Vec::new()).await?;

    // Parse outline
    let outline = parse_outline(&full_content);

    // Collect assets
    let assets = collect_assets(&full_content, source_dir).await;

    state.log.push(format!(
        "[analyzeSource] Parsed {} sections, {} bib files, {} images, {} style files.",
        outline.len(),
        assets.bib_files.len(),
        assets.images.len(),
        assets.style_files.len()
    ));

    state.source_content = full_content;
    state.source_outline = outline;
    state.source_assets = assets;
    state.step = TransferStep::AnalyzeTarget;

    Ok(())
}

async fn step_analyze_target(state: &mut TransferState) -> Result<(), String> {
    let target_dir = Path::new(&state.target_project_dir);

    // Find the main .tex file in the target directory
    let mut target_main = String::new();
    if let Ok(entries) = std::fs::read_dir(target_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tex") {
                target_main = entry.file_name().to_string_lossy().to_string();
                break;
            }
        }
    }

    if target_main.is_empty() {
        // Create a minimal target file
        target_main = "main.tex".to_string();
        let minimal =
            "\\documentclass{article}\n\\begin{document}\n% Content will be placed here\n\\end{document}\n";
        tokio::fs::write(target_dir.join(&target_main), minimal)
            .await
            .map_err(|e| format!("Failed to create target main file: {}", e))?;
    }

    let template_content = tokio::fs::read_to_string(target_dir.join(&target_main))
        .await
        .map_err(|e| format!("Failed to read target template: {}", e))?;

    // Extract preamble (everything before \begin{document})
    let preamble = if let Some(pos) = template_content.find("\\begin{document}") {
        template_content[..pos].to_string()
    } else {
        template_content.clone()
    };

    state.log.push(format!(
        "[analyzeTarget] Read target template: {} ({} chars).",
        target_main,
        template_content.len()
    ));

    state.target_preamble = preamble;
    state.target_template_content = template_content;
    state.step = TransferStep::DraftPlan;

    Ok(())
}

async fn step_draft_plan(state: &mut TransferState) -> Result<(), String> {
    let prompt = format!(
        r#"You are a LaTeX template migration planner.

SOURCE OUTLINE:
{}

TARGET PREAMBLE:
{}

Create a migration plan as a JSON object with:
- "section_mapping": array of {{ "source": "...", "target": "..." }} entries
- "preamble_changes": array of notes about preamble changes needed
- "asset_handling": notes about how to handle images, bib files, etc.

Output ONLY valid JSON."#,
        serde_json::to_string_pretty(&state.source_outline).unwrap_or_default(),
        &state.target_preamble
    );

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: serde_json::Value::String(
                "You are a LaTeX migration expert. Output only valid JSON.".to_string(),
            ),
        },
        Message {
            role: "user".to_string(),
            content: serde_json::Value::String(prompt),
        },
    ];

    let plan = llm::call_llm(&state.llm_config, messages, 0.2).await?;

    state.log.push(format!(
        "[draftPlan] Generated migration plan ({} chars).",
        plan.len()
    ));
    state.transfer_plan = plan;
    state.step = TransferStep::ApplyTransfer;

    Ok(())
}

async fn step_apply_transfer(state: &mut TransferState) -> Result<(), String> {
    let prompt = format!(
        r#"You are a LaTeX template migration expert.

TASK: Migrate the source paper content into the target template structure.

MIGRATION PLAN:
{}

TARGET TEMPLATE (full):
{}

SOURCE CONTENT (full):
{}

RULES:
1. Keep the target preamble (everything before \begin{{document}}) EXACTLY as-is
2. Only modify content between \begin{{document}} and \end{{document}}
3. Follow the section mapping in the migration plan
4. Preserve ALL \cite{{}}, \ref{{}}, \label{{}} commands from the source
5. Preserve ALL figure, table, algorithm environments from the source
6. Adapt section/subsection commands to match the target template style
7. Do NOT add any content that doesn't exist in the source
8. Do NOT remove any substantive content from the source
9. If the source uses \bibliography{{}} but target uses \addbibresource{{}}, adapt accordingly
10. Output the COMPLETE .tex file content, not just the body

Output ONLY the complete LaTeX file content. No explanations, no markdown fences."#,
        &state.transfer_plan, &state.target_template_content, &state.source_content
    );

    let messages = vec![Message {
        role: "user".to_string(),
        content: serde_json::Value::String(prompt),
    }];

    let result = llm::call_llm(&state.llm_config, messages, 0.2).await?;
    let content = strip_code_fences(&result);

    // Find the target main file
    let target_dir = Path::new(&state.target_project_dir);
    let target_main = find_main_tex(target_dir)?;

    tokio::fs::write(target_dir.join(&target_main), &content)
        .await
        .map_err(|e| format!("Failed to write migrated content: {}", e))?;

    state.log.push(format!(
        "[applyTransfer] Wrote migrated content to {} ({} chars).",
        target_main,
        content.len()
    ));
    state.step = TransferStep::CopyAssets;

    Ok(())
}

async fn step_copy_assets(state: &mut TransferState) -> Result<(), String> {
    let source_dir = Path::new(&state.source_project_dir);
    let target_dir = Path::new(&state.target_project_dir);
    let mut copied = 0u32;

    // Copy bib files
    for bib in &state.source_assets.bib_files {
        let src = source_dir.join(bib);
        if src.exists() {
            let dst = target_dir.join(bib);
            if let Some(parent) = dst.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(&src, &dst).is_ok() {
                copied += 1;
            }
        }
    }

    // Copy images
    for img in &state.source_assets.images {
        let src = find_image_file(source_dir, img);
        if let Some(src_path) = src {
            let dst = target_dir.join(img);
            if let Some(parent) = dst.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(&src_path, &dst).is_ok() {
                copied += 1;
            }
        }
    }

    // Copy style files
    for style in &state.source_assets.style_files {
        let src = source_dir.join(style);
        if src.exists() {
            let dst = target_dir.join(
                Path::new(style)
                    .file_name()
                    .unwrap_or(std::ffi::OsStr::new(style)),
            );
            if std::fs::copy(&src, &dst).is_ok() {
                copied += 1;
            }
        }
    }

    state.log.push(format!(
        "[copyAssets] Copied {} asset files to target.",
        copied
    ));
    state.step = TransferStep::Compile;

    Ok(())
}

async fn step_compile(state: &mut TransferState) -> Result<(), String> {
    state.compile_attempt += 1;
    let target_dir = Path::new(&state.target_project_dir);
    let target_main = find_main_tex(target_dir)?;

    // Try to compile using the system's latexmk or pdflatex
    let compile_result = try_compile(target_dir, &target_main).await;

    match compile_result {
        Ok(()) => {
            state.log.push(format!(
                "[compile] Compilation succeeded on attempt {}.",
                state.compile_attempt
            ));
            state.step = TransferStep::Finalize;
        }
        Err(err) => {
            state.log.push(format!(
                "[compile] Attempt {} failed: {}",
                state.compile_attempt,
                &err[..err.len().min(200)]
            ));
            if state.compile_attempt < state.max_compile_attempts {
                state.step = TransferStep::FixCompile;
            } else {
                state.log.push(format!(
                    "[compile] Max attempts ({}) reached. Proceeding to finalize.",
                    state.max_compile_attempts
                ));
                state.step = TransferStep::Finalize;
            }
        }
    }

    Ok(())
}

async fn step_fix_compile(state: &mut TransferState) -> Result<(), String> {
    let target_dir = Path::new(&state.target_project_dir);
    let target_main = find_main_tex(target_dir)?;

    let current_tex = tokio::fs::read_to_string(target_dir.join(&target_main))
        .await
        .map_err(|e| format!("Failed to read current tex: {}", e))?;

    // Read the compile log
    let log_content = read_compile_log(target_dir, &target_main);
    let log_tail: String = log_content
        .chars()
        .rev()
        .take(8000)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    let prompt = format!(
        r#"You are a LaTeX compilation error fixer.

The following LaTeX file failed to compile. Fix the errors and return the corrected COMPLETE file.

COMPILE LOG (last 8000 chars):
{}

CURRENT FILE ({}):
{}

Common fixes:
- Missing packages: add \usepackage{{...}} in preamble
- Undefined commands: replace with standard alternatives or define them
- Mismatched braces: fix bracket/brace pairing
- Missing files: comment out or remove references to missing files
- Encoding issues: ensure UTF-8 compatibility

Output ONLY the complete corrected LaTeX file. No explanations, no markdown fences."#,
        log_tail, target_main, current_tex
    );

    let messages = vec![Message {
        role: "user".to_string(),
        content: serde_json::Value::String(prompt),
    }];

    let result = llm::call_llm(&state.llm_config, messages, 0.2).await?;
    let fixed = strip_code_fences(&result);

    tokio::fs::write(target_dir.join(&target_main), &fixed)
        .await
        .map_err(|e| format!("Failed to write fixed content: {}", e))?;

    state.log.push(format!(
        "[fixCompile] Applied LLM fix for compile attempt {}.",
        state.compile_attempt
    ));
    state.step = TransferStep::Compile;

    Ok(())
}

async fn step_finalize(state: &mut TransferState) -> Result<(), String> {
    state.log.push("[finalize] Transfer complete.".to_string());
    state.step = TransferStep::Complete;
    Ok(())
}

// --- Helpers ---

/// Recursively resolve \input{} and \include{} references.
async fn resolve_inputs(
    project_root: &Path,
    rel_path: &str,
    visited: &mut Vec<String>,
) -> Result<String, String> {
    if visited.contains(&rel_path.to_string()) {
        return Ok(String::new());
    }
    visited.push(rel_path.to_string());

    let abs_path = project_root.join(rel_path);
    let content = match tokio::fs::read_to_string(&abs_path).await {
        Ok(c) => c,
        Err(_) => return Ok(String::new()),
    };

    let mut result = String::new();
    let mut last_end = 0;

    // Find \input{...} and \include{...}
    let re_str = r"\\(?:input|include)\{([^}]+)\}";
    // Simple regex-like parsing without regex crate
    let mut search_pos = 0;
    loop {
        let input_pos = content[search_pos..].find("\\input{");
        let include_pos = content[search_pos..].find("\\include{");

        let (cmd_start, prefix_len) = match (input_pos, include_pos) {
            (Some(ip), Some(inc)) => {
                if ip <= inc {
                    (search_pos + ip, "\\input{".len())
                } else {
                    (search_pos + inc, "\\include{".len())
                }
            }
            (Some(ip), None) => (search_pos + ip, "\\input{".len()),
            (None, Some(inc)) => (search_pos + inc, "\\include{".len()),
            (None, None) => break,
        };

        let brace_start = cmd_start + prefix_len;
        if let Some(brace_end_rel) = content[brace_start..].find('}') {
            let brace_end = brace_start + brace_end_rel;
            let mut ref_path = content[brace_start..brace_end].trim().to_string();

            // Add .tex extension if missing
            if !ref_path.contains('.') {
                ref_path.push_str(".tex");
            }

            result.push_str(&content[last_end..cmd_start]);
            let child_content =
                Box::pin(resolve_inputs_boxed(project_root, &ref_path, visited)).await?;
            result.push_str(&child_content);

            last_end = brace_end + 1;
            search_pos = last_end;
        } else {
            break;
        }
    }

    result.push_str(&content[last_end..]);
    Ok(result)
}

/// Boxed version for recursive async calls.
async fn resolve_inputs_boxed(
    project_root: &Path,
    rel_path: &str,
    visited: &mut Vec<String>,
) -> Result<String, String> {
    resolve_inputs(project_root, rel_path, visited).await
}

/// Parse section/subsection outline from LaTeX content.
fn parse_outline(content: &str) -> Vec<OutlineEntry> {
    let mut outline = Vec::new();
    let mut search_pos = 0;

    loop {
        // Find \section, \subsection, or \subsubsection
        let mut best: Option<(usize, &str)> = None;
        for cmd in &["\\subsubsection", "\\subsection", "\\section"] {
            if let Some(pos) = content[search_pos..].find(cmd) {
                let abs_pos = search_pos + pos;
                if best.is_none() || abs_pos < best.map(|(p, _)| p).unwrap_or(usize::MAX) {
                    best = Some((abs_pos, cmd));
                }
            }
        }

        let (pos, cmd) = match best {
            Some(b) => b,
            None => break,
        };

        // Skip optional * after the command
        let after_cmd = pos + cmd.len();
        let brace_search_start = if content.as_bytes().get(after_cmd) == Some(&b'*') {
            after_cmd + 1
        } else {
            after_cmd
        };

        // Find the { and matching }
        if let Some(open_rel) = content[brace_search_start..].find('{') {
            let open = brace_search_start + open_rel + 1;
            if let Some(close_rel) = content[open..].find('}') {
                let title = content[open..open + close_rel].trim().to_string();
                let level = cmd
                    .trim_start_matches('\\')
                    .trim_end_matches('*')
                    .to_string();
                outline.push(OutlineEntry { level, title });
                search_pos = open + close_rel + 1;
                continue;
            }
        }
        search_pos = after_cmd;
    }

    outline
}

/// Collect asset references from LaTeX content.
async fn collect_assets(content: &str, project_root: &Path) -> SourceAssets {
    let mut assets = SourceAssets::default();

    // Collect \bibliography{} and \addbibresource{}
    collect_brace_args(content, "\\bibliography", &mut assets.bib_files, true);
    collect_brace_args(content, "\\addbibresource", &mut assets.bib_files, false);

    // Add .bib extension if missing
    for bib in &mut assets.bib_files {
        if !bib.contains('.') {
            bib.push_str(".bib");
        }
    }

    // Collect \includegraphics paths
    collect_includegraphics(content, &mut assets.images);

    // Collect style files from directory listing
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".sty") || name.ends_with(".cls") || name.ends_with(".bst") {
                assets.style_files.push(name);
            }
        }
    }

    assets
}

/// Extract arguments from LaTeX commands like \command{arg1,arg2}.
fn collect_brace_args(content: &str, command: &str, out: &mut Vec<String>, split_comma: bool) {
    let mut pos = 0;
    while let Some(cmd_pos) = content[pos..].find(command) {
        let abs = pos + cmd_pos + command.len();
        if let Some(open_rel) = content[abs..].find('{') {
            let open = abs + open_rel + 1;
            if let Some(close_rel) = content[open..].find('}') {
                let arg = &content[open..open + close_rel];
                if split_comma {
                    for part in arg.split(',') {
                        let trimmed = part.trim().to_string();
                        if !trimmed.is_empty() {
                            out.push(trimmed);
                        }
                    }
                } else {
                    let trimmed = arg.trim().to_string();
                    if !trimmed.is_empty() {
                        out.push(trimmed);
                    }
                }
                pos = open + close_rel + 1;
                continue;
            }
        }
        pos = abs;
    }
}

/// Extract \includegraphics paths (handles optional [...] arguments).
fn collect_includegraphics(content: &str, out: &mut Vec<String>) {
    let cmd = "\\includegraphics";
    let mut pos = 0;
    while let Some(cmd_pos) = content[pos..].find(cmd) {
        let abs = pos + cmd_pos + cmd.len();
        let mut brace_start = abs;

        // Skip optional [...] arguments
        if content.as_bytes().get(brace_start) == Some(&b'[') {
            if let Some(close) = content[brace_start..].find(']') {
                brace_start = brace_start + close + 1;
            }
        }

        if content.as_bytes().get(brace_start) == Some(&b'{') {
            let open = brace_start + 1;
            if let Some(close_rel) = content[open..].find('}') {
                let path = content[open..open + close_rel].trim().to_string();
                if !path.is_empty() {
                    out.push(path);
                }
                pos = open + close_rel + 1;
                continue;
            }
        }
        pos = abs;
    }
}

/// Find the main .tex file in a directory.
fn find_main_tex(dir: &Path) -> Result<String, String> {
    // Prefer main.tex, then document.tex, then any .tex file
    for candidate in &["main.tex", "document.tex"] {
        if dir.join(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".tex") {
                return Ok(name);
            }
        }
    }
    Err("No .tex file found in target directory".to_string())
}

/// Find an image file, trying common extensions if the path has none.
fn find_image_file(source_dir: &Path, img_path: &str) -> Option<PathBuf> {
    let full = source_dir.join(img_path);
    if full.exists() {
        return Some(full);
    }
    // Try common extensions
    for ext in &["png", "jpg", "jpeg", "pdf", "eps", "svg"] {
        let with_ext = source_dir.join(format!("{}.{}", img_path, ext));
        if with_ext.exists() {
            return Some(with_ext);
        }
    }
    None
}

/// Try to compile a LaTeX project using system tools.
async fn try_compile(project_dir: &Path, main_file: &str) -> Result<(), String> {
    let dir = project_dir.to_path_buf();
    let file = main_file.to_string();

    tokio::task::spawn_blocking(move || {
        // Try pdflatex first, then latexmk
        let compilers = if cfg!(windows) {
            vec![("pdflatex", vec!["-interaction=nonstopmode", &file])]
        } else {
            vec![
                (
                    "latexmk",
                    vec!["-pdf", "-interaction=nonstopmode", "-halt-on-error", &file],
                ),
                ("pdflatex", vec!["-interaction=nonstopmode", &file]),
            ]
        };

        for (compiler, args) in &compilers {
            if which::which(compiler).is_ok() {
                let output = std::process::Command::new(compiler)
                    .args(args)
                    .current_dir(&dir)
                    .output()
                    .map_err(|e| format!("Failed to run {}: {}", compiler, e))?;

                if output.status.success() {
                    return Ok(());
                }

                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Err(format!(
                    "{} failed:\n{}{}",
                    compiler,
                    if stderr.is_empty() { "" } else { &stderr },
                    if stdout.is_empty() { "" } else { &stdout },
                ));
            }
        }

        Err("No LaTeX compiler found (pdflatex or latexmk). Install a TeX distribution.".to_string())
    })
    .await
    .map_err(|e| format!("Compile task panicked: {}", e))?
}

/// Read the compile log for a given main file.
fn read_compile_log(project_dir: &Path, main_file: &str) -> String {
    let stem = Path::new(main_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let log_path = project_dir.join(format!("{}.log", stem));
    std::fs::read_to_string(log_path).unwrap_or_default()
}

/// Strip markdown code fences from LLM output.
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let after_tag = if let Some(newline_pos) = rest.find('\n') {
            &rest[newline_pos + 1..]
        } else {
            rest
        };
        if let Some(content) = after_tag.strip_suffix("```") {
            return content.trim().to_string();
        }
        return after_tag.trim().to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_outline() {
        let content = r#"\section{Introduction}
Some text here.
\subsection{Background}
More text.
\section{Methods}
\subsubsection{Details}"#;
        let outline = parse_outline(content);
        assert_eq!(outline.len(), 4);
        assert_eq!(outline[0].level, "section");
        assert_eq!(outline[0].title, "Introduction");
        assert_eq!(outline[1].level, "subsection");
        assert_eq!(outline[1].title, "Background");
        assert_eq!(outline[2].level, "section");
        assert_eq!(outline[2].title, "Methods");
        assert_eq!(outline[3].level, "subsubsection");
        assert_eq!(outline[3].title, "Details");
    }

    #[test]
    fn test_parse_outline_starred() {
        let content = r#"\section*{Acknowledgments}"#;
        let outline = parse_outline(content);
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0].title, "Acknowledgments");
    }

    #[test]
    fn test_collect_includegraphics() {
        let content =
            r#"\includegraphics[width=0.5\textwidth]{figures/diagram.png}\includegraphics{logo}"#;
        let mut images = Vec::new();
        collect_includegraphics(content, &mut images);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0], "figures/diagram.png");
        assert_eq!(images[1], "logo");
    }

    #[test]
    fn test_collect_brace_args_bibliography() {
        let content = r#"\bibliography{refs,papers}"#;
        let mut out = Vec::new();
        collect_brace_args(content, "\\bibliography", &mut out, true);
        assert_eq!(out, vec!["refs", "papers"]);
    }

    #[test]
    fn test_strip_code_fences() {
        assert_eq!(
            strip_code_fences("```latex\ncontent\n```"),
            "content"
        );
        assert_eq!(strip_code_fences("no fences"), "no fences");
    }
}
