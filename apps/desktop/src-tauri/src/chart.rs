use std::path::Path;

use crate::llm::{self, LlmConfig, Message};

/// Generate a chart image from LaTeX table data using LLM-generated Python code.
///
/// 1. Calls LLM to generate matplotlib Python code from the table data.
/// 2. Writes the Python script to a temp file.
/// 3. Executes it with `python3` (or `python` on Windows).
/// 4. Saves the output image to `project_dir/assets/charts/`.
/// 5. Returns the relative asset path.
///
/// Retries up to 2 times on failure, feeding the error back to the LLM.
#[tauri::command]
pub async fn generate_chart(
    project_dir: String,
    table_latex: String,
    chart_type: String,
    title: String,
    custom_prompt: Option<String>,
    llm_config: LlmConfig,
) -> Result<String, String> {
    if table_latex.is_empty() {
        return Err("Missing table LaTeX data".to_string());
    }

    let charts_dir = Path::new(&project_dir).join("assets").join("charts");
    std::fs::create_dir_all(&charts_dir)
        .map_err(|e| format!("Failed to create charts directory: {}", e))?;

    let filename = format!("chart_{}.png", uuid::Uuid::new_v4());
    let output_path = charts_dir.join(&filename);
    let asset_rel = format!("assets/charts/{}", filename);

    let system_prompt = [
        "You generate python plotting code using matplotlib (and seaborn if available).",
        "You are given variables: rows (list of rows), header (list of column names),",
        "df (pandas DataFrame or None), df_numeric (numeric DataFrame or None),",
        "plt (matplotlib.pyplot), sns (seaborn or None).",
        "Do NOT import any packages. Do NOT call plt.savefig() or plt.show().",
        "Return ONLY valid Python code, no explanations, no markdown fences.",
    ]
    .join(" ");

    let max_retries = 2;
    let mut last_error = String::new();
    let mut last_code = String::new();

    for attempt in 0..=max_retries {
        // Build user prompt
        let mut user_parts = vec![format!("chart_type: {}", chart_type)];
        if let Some(ref prompt) = custom_prompt {
            if !prompt.trim().is_empty() {
                user_parts.push(format!("user_prompt: {}", prompt));
            }
        }
        if !last_error.is_empty() {
            user_parts.push(format!("runtime_error:\n{}", last_error));
        }
        if !last_code.is_empty() {
            user_parts.push(format!("previous_code:\n{}", last_code));
        }

        let messages = vec![
            Message {
                role: "system".to_string(),
                content: serde_json::Value::String(system_prompt.clone()),
            },
            Message {
                role: "user".to_string(),
                content: serde_json::Value::String(user_parts.join("\n")),
            },
        ];

        let code_result = llm::call_llm(&llm_config, messages, 0.2).await?;

        // Strip code fences
        let plot_code = code_result
            .replace("```python", "")
            .replace("```", "")
            .trim()
            .to_string();

        if plot_code.is_empty() {
            last_error = "LLM returned empty code".to_string();
            continue;
        }

        last_code = plot_code.clone();

        // Build the full Python script
        let python_script = build_python_script(&table_latex, &chart_type, &title, &output_path, &plot_code);

        // Execute the script
        match run_python_script(&python_script).await {
            Ok(()) => {
                if output_path.exists() {
                    return Ok(asset_rel);
                }
                last_error = "Python completed but no output image was generated".to_string();
            }
            Err(err) => {
                last_error = err;
                eprintln!(
                    "[chart] Attempt {}/{} failed: {}",
                    attempt + 1,
                    max_retries + 1,
                    &last_error
                );
            }
        }
    }

    Err(format!("Chart generation failed after {} attempts: {}", max_retries + 1, last_error))
}

/// Build the complete Python script that parses the table and executes LLM-generated plot code.
fn build_python_script(
    table_latex: &str,
    chart_type: &str,
    title: &str,
    output_path: &Path,
    plot_code: &str,
) -> String {
    let escaped_latex = table_latex.replace('\\', "\\\\").replace('\"', "\\\"").replace('\n', "\\n");
    let escaped_title = title.replace('\"', "\\\"");
    let escaped_output = output_path.to_string_lossy().replace('\\', "/");

    format!(
        r#"import os, sys, re, importlib.util
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HAS_PANDAS = importlib.util.find_spec("pandas") is not None
HAS_SEABORN = importlib.util.find_spec("seaborn") is not None
pd = None
sns = None
if HAS_PANDAS:
    import pandas as pd
if HAS_SEABORN:
    import seaborn as sns

def parse_table(latex):
    m = re.search(r'\\begin{{tabular}}.*?\\end{{tabular}}', latex, re.S)
    if m:
        latex = m.group(0)
    latex = re.sub(r'\\begin{{tabular}}{{[^}}]*}}', '', latex)
    latex = re.sub(r'\\end{{tabular}}', '', latex)
    latex = re.sub(r'\\(toprule|midrule|bottomrule|hline)', '', latex)
    rows = [r.strip() for r in latex.split('\\\\') if r.strip()]
    data = []
    for row in rows:
        if row.strip().startswith('%'):
            continue
        raw_cells = [c.strip() for c in re.split(r'(?<!\\)&', row)]
        if len(raw_cells) == 1 and raw_cells[0] == '':
            continue
        data.append(raw_cells)
    return data

def is_number(val):
    try:
        float(val)
        return True
    except Exception:
        return False

latex = "{escaped_latex}"
chart_type = "{chart_type}"
title = "{escaped_title}"
output_path = "{escaped_output}"

data = parse_table(latex)
if not data:
    raise RuntimeError('No table rows parsed')

max_cols = max(len(r) for r in data)
if all(not is_number(c) for c in data[0]):
    header = data[0]
    rows = data[1:]
else:
    header = [f'col{{i+1}}' for i in range(max_cols)]
    rows = data

if len(header) < max_cols:
    header = header + [f'col{{i+1}}' for i in range(len(header), max_cols)]
elif len(header) > max_cols:
    header = header[:max_cols]

if not rows:
    raise RuntimeError('No data rows after header')

rows = [(r + [''] * (max_cols - len(r)))[:max_cols] for r in rows]

plt.figure(figsize=(6, 4))
df = None
df_numeric = None
if HAS_PANDAS:
    df = pd.DataFrame(rows, columns=header)
    df_numeric = df.apply(lambda col: pd.to_numeric(col, errors='coerce'))

{plot_code}

if title:
    plt.title(title)
plt.tight_layout()
os.makedirs(os.path.dirname(output_path), exist_ok=True)
plt.savefig(output_path, dpi=150)
"#
    )
}

/// Execute a Python script in a temporary file and return success or error.
async fn run_python_script(script: &str) -> Result<(), String> {
    let tmp_dir = std::env::temp_dir().join(format!("latexlabs_chart_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let script_path = tmp_dir.join("plot.py");
    std::fs::write(&script_path, script)
        .map_err(|e| format!("Failed to write Python script: {}", e))?;

    let python_bin = resolve_python();

    let script_path_clone = script_path.clone();
    let tmp_dir_clone = tmp_dir.clone();

    let result = tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new(&python_bin)
            .arg(&script_path_clone)
            .current_dir(&tmp_dir_clone)
            .output()
            .map_err(|e| format!("Failed to run Python ({}): {}", python_bin, e))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(format!(
                "{}",
                if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    stderr.to_string()
                }
            ))
        }
    })
    .await
    .map_err(|e| format!("Python task panicked: {}", e))?;

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&tmp_dir);

    result
}

/// Find the Python executable.
fn resolve_python() -> String {
    // Check common Python executable names
    for candidate in &["python3", "python"] {
        if which::which(candidate).is_ok() {
            return candidate.to_string();
        }
    }
    // Fallback
    if cfg!(windows) {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}
