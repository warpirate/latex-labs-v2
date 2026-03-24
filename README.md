<p align="center">
  <img src="./apps/desktop/src-tauri/icons/icon.png" width="120" height="120" alt="LATEX-LABS" />
</p>

<h1 align="center">LATEX-LABS v2</h1>

<p align="center">
  A local-first AI LaTeX IDE for scientific writing.<br/>
  Compile offline. Edit with Claude Code or Codex CLI. Run Python. All from your desktop.
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README.ko.md">한국어</a> ·
  <a href="./README.ja.md">日本語</a> ·
  <a href="./README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <img src="./assets/demo/main.webp" alt="LATEX-LABS v2" width="800" />
</p>

<p align="center">
  <a href="https://github.com/warpirate/latex-labs-v2/releases/latest/download/LATEX-LABS-macOS.dmg">
    <img src="https://img.shields.io/badge/macOS_(Apple_Silicon)-black?style=for-the-badge&logo=apple&logoColor=white" alt="macOS Apple Silicon" />
  </a>&nbsp;
  <a href="https://github.com/warpirate/latex-labs-v2/releases/latest/download/LATEX-LABS-macOS-Intel.dmg">
    <img src="https://img.shields.io/badge/macOS_(Intel)-555555?style=for-the-badge&logo=apple&logoColor=white" alt="macOS Intel" />
  </a>&nbsp;
  <a href="https://github.com/warpirate/latex-labs-v2/releases/latest/download/LATEX-LABS-Windows-setup.exe">
    <img src="https://img.shields.io/badge/Windows-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Windows" />
  </a>&nbsp;
  <a href="https://github.com/warpirate/latex-labs-v2/releases/latest/download/LATEX-LABS-Linux.AppImage">
    <img src="https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux" />
  </a>
</p>

<p align="center">
  <a href="https://github.com/warpirate/latex-labs-v2/releases">
    <img src="https://img.shields.io/github/v/release/warpirate/latex-labs-v2?style=flat-square&label=Latest&color=green" alt="Latest Release" />
  </a>
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT License" />
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform" />
</p>

---

## What is LATEX-LABS?

LATEX-LABS is a desktop application that combines a LaTeX editor, live PDF preview, dual AI assistants, and a Python environment into a single workspace. It integrates both **Claude Code CLI** (Anthropic) and **Codex CLI** (OpenAI) as interchangeable AI backends — switch between them mid-session from the same chat interface. Your files stay on your machine. Compilation happens offline. AI features send prompts to Anthropic or OpenAI APIs for inference when you invoke them.

### Compared to cloud-based alternatives

| | Cloud LaTeX tools | LATEX-LABS |
|---|:---:|:---:|
| Where files live | Their servers | **Your disk** |
| Compilation | Cloud | **Local (Tectonic, offline)** |
| AI Models | Single provider | **Claude (Opus / Sonnet / Haiku) + Codex (o3 / o4-mini / GPT-4.1)** |
| Python | Not available | **Built-in uv + venv** |
| Scientific Skills | Not available | **100+ domain skills** |
| Version Control | Cloud-managed | **Local Git history with diffs** |
| Source | Proprietary | **Open source (MIT)** |

---

## Features

### Editor

CodeMirror 6 with LaTeX and BibTeX syntax highlighting, real-time error linting, regex find & replace, multi-file project support, and auto-save.

### Live PDF Preview

Native MuPDF rendering with SyncTeX — click anywhere in the PDF to jump to the corresponding source line. Supports zoom, text selection, and region capture.

<p align="center">
  <img src="./assets/demo/main.webp" alt="Editor and PDF Preview" width="700" />
</p>

### Dual AI Backend — Claude Code + Codex

LATEX-LABS ships with two AI providers you can switch between at any time:

| Provider | CLI | Models |
|----------|-----|--------|
| **Anthropic** | Claude Code CLI | Opus, Sonnet, Haiku (with adjustable reasoning effort) |
| **OpenAI** | Codex CLI | o3, o4-mini, GPT-4.1 |

Both providers share the same chat interface. They can edit your files, run shell commands, and search your project. Persistent sessions carry context across interactions. When the AI suggests edits, changes appear in a proposed changes panel with visual diffs — accept or reject each chunk individually, or apply/undo all at once with `Cmd+Y` / `Cmd+N`.

<p align="center">
  <img src="./assets/demo/claudecommand.webp" alt="Claude AI Assistant" width="600" />
</p>

### Capture & Ask

Press `Cmd+X` to enter capture mode. Drag to select any region in the PDF — the screenshot is pinned to the chat composer so you can ask the AI about equations, figures, tables, or reviewer comments.

<p align="center">
  <img src="./assets/demo/capture_ask.webp" alt="Capture and Ask" width="700" />
</p>

### Python Environment

One-click [uv](https://docs.astral.sh/uv/) installation and project-level virtual environment setup. Both Claude Code and Codex automatically use the `.venv` when running Python, so you can generate plots, run analysis scripts, and process data without leaving the app.

<p align="center">
  <img src="./assets/demo/python.webp" alt="Python Environment" width="600" />
</p>

### 100+ Scientific Skills

Install domain-specific skill packs that give the AI deep knowledge in specialized fields:

| Domain | Examples |
|--------|----------|
| Bioinformatics & Genomics | Scanpy, BioPython, PyDESeq2, PySAM, gget, AnnData |
| Cheminformatics & Drug Discovery | RDKit, DeepChem, DiffDock, PubChem, ChEMBL |
| Data Analysis & Visualization | Matplotlib, Seaborn, Plotly, Polars, scikit-learn |
| Machine Learning & AI | PyTorch Lightning, Transformers, SHAP, UMAP, PyMC |
| Clinical Research | ClinicalTrials.gov, ClinVar, DrugBank, FDA |
| Scientific Communication | Literature Review, Grant Writing, Citation Management |

Skills are installed globally (`~/.claude/skills/`) or per-project and loaded automatically when relevant.

<p align="center">
  <img src="./assets/demo/scientific.webp" alt="Scientific Skills" width="700" />
</p>

### Project Templates

Pick from paper, thesis, presentation, poster, letter, and other templates. Optionally describe what you're writing and let AI generate the initial structure. Drag & drop PDFs, BIB files, and images as references.

<p align="center">
  <img src="./assets/demo/starter.webp" alt="Templates and Project Wizard" width="700" />
</p>

### History & Version Control

Every save creates a Git snapshot in `.latexlabs/history.git/`. Label important checkpoints, browse diffs between any two versions, and restore previous states.

<p align="center">
  <img src="./assets/demo/history.webp" alt="History and Diffs" width="700" />
</p>

### More

- **Zotero Integration** — OAuth-based bibliography management and citation insertion
- **ArXiv Search** — Search and browse papers directly in the app
- **Vision Panel** — Analyze images with AI (equations, tables, figures, OCR)
- **Slash Commands** — Built-in (`/review`, `/init`) + custom commands from `.claude/commands/`
- **External Editors** — Open projects in VS Code, Cursor, Zed, or Sublime Text
- **Dark / Light Theme** — Automatic or manual switching

<p align="center">
  <img src="./assets/demo/zotero.webp" alt="Zotero Integration" width="300" />
</p>

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Framework | Tauri 2 (Rust backend) |
| Frontend | React 19, TypeScript, Vite |
| Editor | CodeMirror 6 |
| PDF | MuPDF (native) + SyncTeX |
| State | Zustand |
| Styling | Tailwind CSS 4 |
| LaTeX | Tectonic / pdflatex / xelatex |
| Version Control | libgit2 (via git2-rs) |
| AI (Anthropic) | Claude Code CLI |
| AI (OpenAI) | Codex CLI |
| Python | uv + venv |
| Monorepo | Turborepo + pnpm |

---

## Getting Started

1. Download the installer for your platform from [Releases](https://github.com/warpirate/latex-labs-v2/releases)
2. Launch LATEX-LABS and create or open a project
3. Install [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) and/or [Codex CLI](https://github.com/openai/codex) to enable AI features
4. Start writing

### Building from Source

```bash
git clone https://github.com/warpirate/latex-labs-v2.git
cd latex-labs-v2
pnpm install
pnpm dev:desktop
```

Requires: Node.js, pnpm, Rust toolchain, and [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

---

## Data & Privacy

LATEX-LABS stores and compiles everything locally. Nothing is uploaded for storage. When you use AI features, prompts and file contents are sent to Anthropic's API (or OpenAI for Codex) for inference. See [Claude Code data usage](https://code.claude.com/docs/en/data-usage) for retention policies.

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup and guidelines.

## Acknowledgments

Forked from [LATEX-LABS](https://github.com/delibae/latex-labs) by [delibae](https://github.com/delibae), originally built on [Open Prism](https://github.com/assistant-ui/open-prism) by [assistant-ui](https://github.com/assistant-ui).

## License

[MIT](./LICENSE)
