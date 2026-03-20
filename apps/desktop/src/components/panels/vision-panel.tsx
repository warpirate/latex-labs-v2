import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  Image,
  Upload,
  Copy,
  Check,
  Loader2,
  AlertCircle,
  TextCursorInput,
  Sigma,
  Table2,
  ImageIcon,
  Code2,
  ScanText,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useDocumentStore } from "@/stores/document-store";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface LlmConfig {
  endpoint: string;
  apiKey: string;
  model: string;
}

type VisionMode = "equation" | "table" | "figure" | "algorithm" | "ocr";

interface ModeOption {
  value: VisionMode;
  label: string;
  icon: React.ReactNode;
}

const VISION_MODES: ModeOption[] = [
  { value: "equation", label: "Equation", icon: <Sigma className="size-3.5" /> },
  { value: "table", label: "Table", icon: <Table2 className="size-3.5" /> },
  { value: "figure", label: "Figure", icon: <ImageIcon className="size-3.5" /> },
  { value: "algorithm", label: "Algorithm", icon: <Code2 className="size-3.5" /> },
  { value: "ocr", label: "OCR", icon: <ScanText className="size-3.5" /> },
];

const LLM_CONFIG_KEY = "latexlabs-llm-config";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getLlmConfig(): LlmConfig | null {
  try {
    const raw = localStorage.getItem(LLM_CONFIG_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<LlmConfig>;
    if (!parsed.apiKey) return null;
    return {
      endpoint: parsed.endpoint ?? "",
      apiKey: parsed.apiKey,
      model: parsed.model ?? "",
    };
  } catch {
    return null;
  }
}

/** Convert a Uint8Array to a base64 string (no data-url prefix). */
function uint8ToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

/** Detect MIME type from file extension. */
function mimeFromExt(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "png";
  const map: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    bmp: "image/bmp",
  };
  return map[ext] ?? "image/png";
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function VisionPanel() {
  const [mode, setMode] = useState<VisionMode>("equation");
  const [imageBase64, setImageBase64] = useState<string | null>(null);
  const [imagePreviewUrl, setImagePreviewUrl] = useState<string | null>(null);
  const [customPrompt, setCustomPrompt] = useState("");
  const [result, setResult] = useState<string | null>(null);
  const [isConverting, setIsConverting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);

  const dropRef = useRef<HTMLDivElement>(null);
  const insertAtCursor = useDocumentStore((s) => s.insertAtCursor);

  // -- Load image from file path ------------------------------------------
  const loadImageFromPath = useCallback(async (filePath: string) => {
    try {
      const bytes = await readFile(filePath);
      const base64 = uint8ToBase64(new Uint8Array(bytes));
      const mime = mimeFromExt(filePath);
      setImageBase64(base64);
      setImagePreviewUrl(`data:${mime};base64,${base64}`);
      setError(null);
      setResult(null);
    } catch (e) {
      setError(`Failed to read image: ${e}`);
    }
  }, []);

  // -- Load image from browser File object --------------------------------
  const loadImageFromFile = useCallback((file: File) => {
    if (!file.type.startsWith("image/")) {
      setError("Selected file is not an image.");
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const dataUrl = reader.result as string;
      // dataUrl format: "data:<mime>;base64,<data>"
      const base64 = dataUrl.split(",")[1];
      setImageBase64(base64);
      setImagePreviewUrl(dataUrl);
      setError(null);
      setResult(null);
    };
    reader.onerror = () => setError("Failed to read the image file.");
    reader.readAsDataURL(file);
  }, []);

  // -- Browse for image via Tauri dialog ----------------------------------
  const handleBrowse = useCallback(async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [
        {
          name: "Images",
          extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
        },
      ],
    });
    if (selected) {
      await loadImageFromPath(selected);
    }
  }, [loadImageFromPath]);

  // -- Paste handler (Ctrl+V) --------------------------------------------
  const handlePaste = useCallback(
    (e: React.ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) {
            loadImageFromFile(file);
            e.preventDefault();
            return;
          }
        }
      }
    },
    [loadImageFromFile],
  );

  // -- Drag & Drop --------------------------------------------------------
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragOver(false);

      const files = e.dataTransfer?.files;
      if (files && files.length > 0) {
        loadImageFromFile(files[0]);
      }
    },
    [loadImageFromFile],
  );

  // -- Convert ------------------------------------------------------------
  const handleConvert = useCallback(async () => {
    if (!imageBase64) {
      setError("Please upload an image first.");
      return;
    }

    const llmConfig = getLlmConfig();
    if (!llmConfig) {
      setError(
        "LLM is not configured. Please set your API key in Settings.",
      );
      return;
    }

    setIsConverting(true);
    setError(null);
    setResult(null);

    try {
      const latex = await invoke<string>("vision_to_latex", {
        imageBase64,
        mode,
        customPrompt: customPrompt.trim() || null,
        llmConfig: {
          endpoint: llmConfig.endpoint || null,
          api_key: llmConfig.apiKey,
          model: llmConfig.model || null,
        },
      });
      setResult(latex);
    } catch (e) {
      setError(`Conversion failed: ${e}`);
    } finally {
      setIsConverting(false);
    }
  }, [imageBase64, mode, customPrompt]);

  // -- Copy to clipboard --------------------------------------------------
  const handleCopy = useCallback(async () => {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setError("Failed to copy to clipboard.");
    }
  }, [result]);

  // -- Insert at cursor ---------------------------------------------------
  const handleInsert = useCallback(() => {
    if (!result) return;
    insertAtCursor(result);
  }, [result, insertAtCursor]);

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <div
      className="flex h-full flex-col bg-[var(--background)]"
      onPaste={handlePaste}
    >
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-[var(--border)] px-3 py-2">
        <Image className="size-4 text-[var(--primary)]" />
        <span className="text-sm font-medium text-[var(--foreground)]">
          Vision to LaTeX
        </span>
      </div>

      <ScrollArea className="flex-1">
        <div className="flex flex-col gap-3 p-3">
          {/* Mode selector */}
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-[var(--foreground)] opacity-70">
              Mode
            </label>
            <div className="flex gap-1">
              {VISION_MODES.map((m) => (
                <button
                  key={m.value}
                  onClick={() => setMode(m.value)}
                  className={cn(
                    "flex flex-1 items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium transition-colors",
                    mode === m.value
                      ? "bg-[var(--primary)] text-white"
                      : "bg-[var(--secondary)] text-[var(--foreground)] hover:bg-[var(--secondary)]/80",
                  )}
                >
                  {m.icon}
                  <span className="hidden sm:inline">{m.label}</span>
                </button>
              ))}
            </div>
          </div>

          {/* Image upload area */}
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-[var(--foreground)] opacity-70">
              Image
            </label>
            <div
              ref={dropRef}
              onClick={handleBrowse}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              className={cn(
                "flex min-h-[120px] cursor-pointer flex-col items-center justify-center rounded-md border-2 border-dashed transition-colors",
                isDragOver
                  ? "border-[var(--primary)] bg-[var(--primary)]/10"
                  : "border-[var(--border)] bg-[var(--secondary)]/50 hover:border-[var(--primary)]/50",
              )}
            >
              {imagePreviewUrl ? (
                <img
                  src={imagePreviewUrl}
                  alt="Uploaded preview"
                  className="max-h-[200px] max-w-full rounded object-contain p-2"
                />
              ) : (
                <div className="flex flex-col items-center gap-2 p-4 text-center">
                  <Upload className="size-6 text-[var(--foreground)] opacity-40" />
                  <span className="text-xs text-[var(--foreground)] opacity-60">
                    Click to browse, drag & drop, or paste (Ctrl+V)
                  </span>
                </div>
              )}
            </div>
          </div>

          {/* Custom prompt */}
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-[var(--foreground)] opacity-70">
              Custom prompt (optional)
            </label>
            <Textarea
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              placeholder="e.g. Use align environment instead of equation..."
              className="min-h-[48px] resize-none border-[var(--border)] bg-[var(--secondary)] text-xs text-[var(--foreground)] placeholder:text-[var(--foreground)]/30"
              rows={2}
            />
          </div>

          {/* Convert button */}
          <Button
            onClick={handleConvert}
            disabled={!imageBase64 || isConverting}
            className="w-full bg-[var(--primary)] text-white hover:bg-[var(--primary)]/90 disabled:opacity-50"
          >
            {isConverting ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                Converting...
              </>
            ) : (
              <>
                <TextCursorInput className="size-4" />
                Convert to LaTeX
              </>
            )}
          </Button>

          {/* Error display */}
          {error && (
            <div className="flex items-start gap-2 rounded-md border border-red-500/30 bg-red-500/10 p-2.5">
              <AlertCircle className="mt-0.5 size-4 shrink-0 text-red-400" />
              <span className="text-xs text-red-300">{error}</span>
            </div>
          )}

          {/* Results area */}
          {result && (
            <div className="flex flex-col gap-1.5">
              <div className="flex items-center justify-between">
                <label className="text-xs font-medium text-[var(--foreground)] opacity-70">
                  Result
                </label>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={handleCopy}
                    className="text-[var(--foreground)] opacity-70 hover:opacity-100"
                  >
                    {copied ? (
                      <Check className="size-3 text-green-400" />
                    ) : (
                      <Copy className="size-3" />
                    )}
                    <span>{copied ? "Copied" : "Copy"}</span>
                  </Button>
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={handleInsert}
                    className="text-[var(--foreground)] opacity-70 hover:opacity-100"
                  >
                    <TextCursorInput className="size-3" />
                    <span>Insert</span>
                  </Button>
                </div>
              </div>
              <div className="rounded-md border border-[var(--border)] bg-[var(--secondary)] p-3">
                <pre className="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed text-[var(--foreground)]">
                  {result}
                </pre>
              </div>
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
