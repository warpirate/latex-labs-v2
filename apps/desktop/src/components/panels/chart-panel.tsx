import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  BarChart3,
  LineChart,
  ScatterChart,
  PieChart,
  BarChart,
  Grid3x3,
  FileDown,
  Image,
  Loader2,
  AlertCircle,
  TableProperties,
  Sparkles,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { useDocumentStore } from "@/stores/document-store";

// ── Types ──

type ChartType = "bar" | "line" | "scatter" | "pie" | "histogram" | "heatmap";

interface LlmConfig {
  provider?: string;
  model?: string;
  apiKey?: string;
  endpoint?: string;
}

interface GenerateChartResult {
  imagePath: string;
  imageBase64?: string;
}

// ── Chart type metadata ──

const CHART_TYPES: { value: ChartType; label: string; icon: React.ReactNode }[] = [
  { value: "bar", label: "Bar", icon: <BarChart3 className="size-4" /> },
  { value: "line", label: "Line", icon: <LineChart className="size-4" /> },
  { value: "scatter", label: "Scatter", icon: <ScatterChart className="size-4" /> },
  { value: "pie", label: "Pie", icon: <PieChart className="size-4" /> },
  { value: "histogram", label: "Histogram", icon: <BarChart className="size-4" /> },
  { value: "heatmap", label: "Heatmap", icon: <Grid3x3 className="size-4" /> },
];

// ── Helpers ──

function readLlmConfig(): LlmConfig {
  try {
    const raw = localStorage.getItem("latexlabs-llm-config");
    if (!raw) return {};
    return JSON.parse(raw) as LlmConfig;
  } catch {
    return {};
  }
}

/**
 * Extract the first \begin{tabular}...\end{tabular} block from LaTeX source.
 * Also handles tabular*, longtable, and tabularx variants.
 */
function extractTabularFromLatex(content: string): string | null {
  const pattern = /\\begin\{(tabular[*x]?|longtable)\}[\s\S]*?\\end\{\1\}/;
  const match = content.match(pattern);
  return match ? match[0] : null;
}

// ── Component ──

export function ChartPanel() {
  const projectRoot = useDocumentStore((s) => s.projectRoot);
  const files = useDocumentStore((s) => s.files);
  const activeFileId = useDocumentStore((s) => s.activeFileId);
  const insertAtCursor = useDocumentStore((s) => s.insertAtCursor);

  const [tableLatex, setTableLatex] = useState("");
  const [chartType, setChartType] = useState<ChartType>("bar");
  const [title, setTitle] = useState("");
  const [customPrompt, setCustomPrompt] = useState("");
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previewSrc, setPreviewSrc] = useState<string | null>(null);
  const [generatedImagePath, setGeneratedImagePath] = useState<string | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);

  // Extract tabular block from active editor content
  const handleExtractFromDocument = useCallback(() => {
    const activeFile = files.find((f) => f.id === activeFileId);
    if (!activeFile?.content) {
      setError("No active document content found.");
      return;
    }

    const tabular = extractTabularFromLatex(activeFile.content);
    if (tabular) {
      setTableLatex(tabular);
      setError(null);
    } else {
      setError(
        "No \\begin{tabular} block found in the current document. Paste your table LaTeX manually.",
      );
    }
  }, [files, activeFileId]);

  // Generate chart via Tauri command
  const handleGenerate = useCallback(async () => {
    if (!projectRoot) {
      setError("No project is open.");
      return;
    }
    if (!tableLatex.trim()) {
      setError("Please provide LaTeX table code.");
      return;
    }

    setIsGenerating(true);
    setError(null);
    setPreviewSrc(null);
    setGeneratedImagePath(null);

    try {
      const llmConfig = readLlmConfig();

      const result = await invoke<GenerateChartResult>("generate_chart", {
        projectDir: projectRoot,
        tableLatex: tableLatex.trim(),
        chartType,
        title: title.trim() || undefined,
        customPrompt: customPrompt.trim() || undefined,
        llmConfig: Object.keys(llmConfig).length > 0 ? llmConfig : undefined,
      });

      setGeneratedImagePath(result.imagePath);

      if (result.imageBase64) {
        setPreviewSrc(`data:image/png;base64,${result.imageBase64}`);
      } else if (result.imagePath) {
        // Use Tauri's convertFileSrc or a file:// path for local preview
        setPreviewSrc(`file://${result.imagePath}`);
      }
    } catch (err) {
      const message =
        typeof err === "string" ? err : err instanceof Error ? err.message : "Chart generation failed.";
      setError(message);
    } finally {
      setIsGenerating(false);
    }
  }, [projectRoot, tableLatex, chartType, title, customPrompt]);

  // Insert \includegraphics into the editor
  const handleInsertIntoDocument = useCallback(() => {
    if (!generatedImagePath) return;

    // Extract just the filename relative to the project
    let relativePath = generatedImagePath;
    if (projectRoot && generatedImagePath.startsWith(projectRoot)) {
      relativePath = generatedImagePath
        .slice(projectRoot.length)
        .replace(/^[/\\]+/, "");
    }
    // Remove extension for \includegraphics
    const pathWithoutExt = relativePath.replace(/\.[^.]+$/, "");

    const latex = [
      "\\begin{figure}[htbp]",
      "  \\centering",
      `  \\includegraphics[width=0.8\\textwidth]{${pathWithoutExt}}`,
      title ? `  \\caption{${title}}` : "  \\caption{}",
      "  \\label{fig:chart}",
      "\\end{figure}",
    ].join("\n");

    insertAtCursor(`\n${latex}\n`);
  }, [generatedImagePath, projectRoot, title, insertAtCursor]);

  const handleClearPreview = useCallback(() => {
    setPreviewSrc(null);
    setGeneratedImagePath(null);
    setError(null);
  }, []);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-border px-4 py-3">
        <BarChart3 className="size-4 text-primary" />
        <h2 className="text-sm font-semibold text-foreground">Chart Generator</h2>
      </div>

      <ScrollArea className="flex-1">
        <div ref={scrollRef} className="space-y-4 p-4">
          {/* Table LaTeX input */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="text-xs text-muted-foreground">
                LaTeX Table Code
              </Label>
              <Button
                variant="ghost"
                size="xs"
                onClick={handleExtractFromDocument}
                className="gap-1 text-xs text-primary"
              >
                <TableProperties className="size-3" />
                Extract from document
              </Button>
            </div>
            <Textarea
              value={tableLatex}
              onChange={(e) => {
                setTableLatex(e.target.value);
                if (error) setError(null);
              }}
              placeholder={`Paste your LaTeX table here, e.g.:\n\\begin{tabular}{lcc}\n  \\hline\n  Category & Value A & Value B \\\\\n  \\hline\n  Item 1 & 10 & 20 \\\\\n  Item 2 & 30 & 15 \\\\\n  \\hline\n\\end{tabular}`}
              className="min-h-32 font-mono text-xs"
            />
          </div>

          {/* Chart type selector */}
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">Chart Type</Label>
            <div className="grid grid-cols-3 gap-1.5">
              {CHART_TYPES.map((ct) => (
                <button
                  key={ct.value}
                  onClick={() => setChartType(ct.value)}
                  className={`flex items-center gap-1.5 rounded-md border px-2.5 py-2 text-xs font-medium transition-colors ${
                    chartType === ct.value
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-border bg-background text-muted-foreground hover:border-ring hover:text-foreground"
                  }`}
                >
                  {ct.icon}
                  {ct.label}
                </button>
              ))}
            </div>
          </div>

          {/* Title input */}
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">
              Chart Title
            </Label>
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. Performance Comparison"
              className="text-sm"
            />
          </div>

          {/* Custom prompt */}
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">
              Custom Prompt{" "}
              <span className="text-muted-foreground/60">(optional)</span>
            </Label>
            <Textarea
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              placeholder="Additional instructions for chart styling, colors, axis labels..."
              className="min-h-16 text-xs"
            />
          </div>

          {/* Generate button */}
          <Button
            onClick={handleGenerate}
            disabled={isGenerating || !tableLatex.trim() || !projectRoot}
            className="w-full gap-2"
          >
            {isGenerating ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                Generating...
              </>
            ) : (
              <>
                <Sparkles className="size-4" />
                Generate Chart
              </>
            )}
          </Button>

          {/* Error display */}
          {error && (
            <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2.5">
              <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
              <p className="text-xs text-destructive">{error}</p>
            </div>
          )}

          {/* Preview area */}
          {previewSrc && (
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-xs text-muted-foreground">
                  Preview
                </Label>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={handleClearPreview}
                >
                  <X className="size-3" />
                </Button>
              </div>
              <div className="overflow-hidden rounded-md border border-border bg-background">
                <img
                  src={previewSrc}
                  alt={title || "Generated chart"}
                  className="w-full object-contain"
                  onError={() => {
                    setError("Failed to load chart preview image.");
                    setPreviewSrc(null);
                  }}
                />
              </div>

              {/* Insert button */}
              <Button
                variant="outline"
                onClick={handleInsertIntoDocument}
                className="w-full gap-2"
              >
                <FileDown className="size-4" />
                Insert into Document
              </Button>

              {generatedImagePath && (
                <div className="flex items-center gap-1.5">
                  <Image className="size-3 text-muted-foreground" />
                  <span className="truncate text-[11px] text-muted-foreground">
                    {generatedImagePath.split(/[/\\]/).pop()}
                  </span>
                </div>
              )}
            </div>
          )}

          {/* No project warning */}
          {!projectRoot && (
            <div className="flex items-center gap-2 rounded-md border border-border bg-muted/50 px-3 py-2.5">
              <AlertCircle className="size-4 shrink-0 text-muted-foreground" />
              <p className="text-xs text-muted-foreground">
                Open a project to generate charts.
              </p>
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
