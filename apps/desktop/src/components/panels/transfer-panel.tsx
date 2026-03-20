import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  ArrowRightLeft,
  Play,
  XCircle,
  Loader2,
  AlertCircle,
  CheckCircle2,
  FileText,
  FolderOpen,
  RefreshCw,
  ChevronRight,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import { useDocumentStore } from "@/stores/document-store";
import {
  getAllTemplates,
  type TemplateDefinition,
  CATEGORY_LABELS,
  type TemplateCategory,
} from "@/lib/template-registry";

// ── Types ──

interface LlmConfig {
  provider?: string;
  model?: string;
  apiKey?: string;
  endpoint?: string;
}

interface TransferStartResult {
  jobId: string;
}

interface TransferProgressEvent {
  jobId: string;
  step: TransferStep;
  stepIndex: number;
  totalSteps: number;
  message: string;
  isError?: boolean;
}

type TransferStep =
  | "analyze_source"
  | "analyze_target"
  | "draft_plan"
  | "apply_transfer"
  | "copy_assets"
  | "compile"
  | "fix_compile"
  | "finalize";

type TransferStatus = "idle" | "running" | "success" | "failed" | "cancelled";

// ── Step metadata ──

const TRANSFER_STEPS: { key: TransferStep; label: string }[] = [
  { key: "analyze_source", label: "Analyze Source" },
  { key: "analyze_target", label: "Analyze Target" },
  { key: "draft_plan", label: "Draft Plan" },
  { key: "apply_transfer", label: "Apply Transfer" },
  { key: "copy_assets", label: "Copy Assets" },
  { key: "compile", label: "Compile" },
  { key: "fix_compile", label: "Fix Errors" },
  { key: "finalize", label: "Finalize" },
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

function getStepIndex(step: TransferStep): number {
  return TRANSFER_STEPS.findIndex((s) => s.key === step);
}

// ── Component ──

export function TransferPanel() {
  const projectRoot = useDocumentStore((s) => s.projectRoot);
  const files = useDocumentStore((s) => s.files);
  const refreshFiles = useDocumentStore((s) => s.refreshFiles);

  // Derive project name and main file from store
  const projectName = projectRoot?.split(/[/\\]/).pop() ?? "Untitled";
  const mainFile =
    files.find((f) => f.name === "main.tex" || f.name === "document.tex") ??
    files.find((f) => f.type === "tex");
  const mainFileName = mainFile?.relativePath ?? "main.tex";

  // Template selection
  const [templates] = useState<TemplateDefinition[]>(() => getAllTemplates());
  const [targetTemplateId, setTargetTemplateId] = useState("");

  // Transfer job state
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<TransferStatus>("idle");
  const [currentStep, setCurrentStep] = useState<TransferStep | null>(null);
  const [currentStepIndex, setCurrentStepIndex] = useState(0);
  const [totalSteps, setTotalSteps] = useState(TRANSFER_STEPS.length);
  const [logs, setLogs] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const logEndRef = useRef<HTMLDivElement>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  // Auto-scroll log viewer
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  // Listen for transfer-progress Tauri events
  useEffect(() => {
    let cancelled = false;

    const setup = async () => {
      const unlisten = await listen<TransferProgressEvent>(
        "transfer-progress",
        (event) => {
          if (cancelled) return;
          const payload = event.payload;

          // Only process events for our job
          if (jobId && payload.jobId !== jobId) return;

          setCurrentStep(payload.step);
          setCurrentStepIndex(payload.stepIndex);
          setTotalSteps(payload.totalSteps || TRANSFER_STEPS.length);
          setLogs((prev) => [...prev, payload.message]);

          if (payload.isError) {
            setError(payload.message);
            setStatus("failed");
          }
        },
      );
      if (!cancelled) {
        unlistenRef.current = unlisten;
      } else {
        unlisten();
      }
    };

    setup();

    return () => {
      cancelled = true;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, [jobId]);

  // Start transfer
  const handleStart = useCallback(async () => {
    if (!projectRoot || !targetTemplateId) return;

    setStatus("running");
    setError(null);
    setLogs([]);
    setCurrentStep(null);
    setCurrentStepIndex(0);

    try {
      const llmConfig = readLlmConfig();

      const result = await invoke<TransferStartResult>("transfer_start", {
        sourceProjectDir: projectRoot,
        sourceMainFile: mainFileName,
        targetTemplateId,
        llmConfig: Object.keys(llmConfig).length > 0 ? llmConfig : undefined,
      });

      setJobId(result.jobId);
      setLogs((prev) => [...prev, `Transfer started (job: ${result.jobId})`]);

      // Listen for completion — the backend sends a final event with step "finalize"
      // We also poll for status in case events are missed
      const checkResult = await invoke<{ status: string; error?: string }>(
        "transfer_await",
        { jobId: result.jobId },
      ).catch(() => null);

      if (checkResult) {
        if (checkResult.status === "success") {
          setStatus("success");
          setLogs((prev) => [...prev, "Transfer completed successfully."]);
          setCurrentStepIndex(TRANSFER_STEPS.length);
          // Refresh project files to pick up transferred content
          await refreshFiles();
        } else if (checkResult.status === "failed") {
          setStatus("failed");
          setError(checkResult.error ?? "Transfer failed.");
          setLogs((prev) => [
            ...prev,
            `Transfer failed: ${checkResult.error ?? "Unknown error"}`,
          ]);
        }
      }
    } catch (err) {
      const message =
        typeof err === "string"
          ? err
          : err instanceof Error
            ? err.message
            : "Failed to start transfer.";
      setError(message);
      setStatus("failed");
      setLogs((prev) => [...prev, `Error: ${message}`]);
    }
  }, [projectRoot, targetTemplateId, mainFileName, refreshFiles]);

  // Cancel transfer
  const handleCancel = useCallback(async () => {
    if (!jobId) return;

    try {
      await invoke("transfer_cancel", { jobId });
      setStatus("cancelled");
      setLogs((prev) => [...prev, "Transfer cancelled by user."]);
    } catch (err) {
      const message =
        typeof err === "string" ? err : "Failed to cancel transfer.";
      setLogs((prev) => [...prev, `Cancel error: ${message}`]);
    }
  }, [jobId]);

  // Reset to idle
  const handleReset = useCallback(() => {
    setStatus("idle");
    setJobId(null);
    setCurrentStep(null);
    setCurrentStepIndex(0);
    setLogs([]);
    setError(null);
  }, []);

  // Retry on failure
  const handleRetry = useCallback(() => {
    handleReset();
    // Slight delay so state clears before re-starting
    setTimeout(() => handleStart(), 50);
  }, [handleReset, handleStart]);

  const progressPercent =
    totalSteps > 0 ? Math.round((currentStepIndex / totalSteps) * 100) : 0;

  const isRunning = status === "running";
  const isIdle = status === "idle";
  const isDone = status === "success" || status === "failed" || status === "cancelled";

  // Group templates by category for the select dropdown
  const templatesByCategory = templates.reduce<
    Record<string, TemplateDefinition[]>
  >((acc, t) => {
    const cat = t.category;
    if (!acc[cat]) acc[cat] = [];
    acc[cat].push(t);
    return acc;
  }, {});

  const selectedTemplate = templates.find((t) => t.id === targetTemplateId);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-border px-4 py-3">
        <ArrowRightLeft className="size-4 text-primary" />
        <h2 className="text-sm font-semibold text-foreground">
          Template Transfer
        </h2>
        {isRunning && (
          <Badge variant="secondary" className="ml-auto text-[10px]">
            In Progress
          </Badge>
        )}
        {status === "success" && (
          <Badge className="ml-auto bg-emerald-500/15 text-[10px] text-emerald-600 dark:text-emerald-400">
            Complete
          </Badge>
        )}
      </div>

      <ScrollArea className="flex-1">
        <div className="space-y-4 p-4">
          {/* Source section */}
          <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
            <Label className="text-xs font-medium text-muted-foreground">
              Source
            </Label>
            <div className="flex items-center gap-2">
              <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium text-foreground">
                  {projectName}
                </p>
                <div className="flex items-center gap-1">
                  <FileText className="size-3 text-muted-foreground" />
                  <span className="truncate text-xs text-muted-foreground">
                    {mainFileName}
                  </span>
                </div>
              </div>
            </div>
          </div>

          {/* Arrow indicator */}
          <div className="flex justify-center">
            <div className="rounded-full border border-border bg-background p-1.5">
              <ChevronRight className="size-3.5 rotate-90 text-muted-foreground" />
            </div>
          </div>

          {/* Target template selector */}
          <div className="space-y-2">
            <Label className="text-xs text-muted-foreground">
              Target Template
            </Label>
            <Select
              value={targetTemplateId}
              onValueChange={setTargetTemplateId}
              disabled={isRunning}
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Select a template..." />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(templatesByCategory).map(([category, tmpls]) => (
                  <div key={category}>
                    <div className="px-2 py-1.5 text-xs font-semibold text-muted-foreground">
                      {CATEGORY_LABELS[category as TemplateCategory] ?? category}
                    </div>
                    {tmpls.map((tmpl) => (
                      <SelectItem key={tmpl.id} value={tmpl.id}>
                        {tmpl.name}
                      </SelectItem>
                    ))}
                  </div>
                ))}
              </SelectContent>
            </Select>

            {selectedTemplate && (
              <p className="text-[11px] leading-relaxed text-muted-foreground">
                {selectedTemplate.description}
              </p>
            )}
          </div>

          {/* Action buttons */}
          {isIdle && (
            <Button
              onClick={handleStart}
              disabled={!projectRoot || !targetTemplateId}
              className="w-full gap-2"
            >
              <Play className="size-4" />
              Start Transfer
            </Button>
          )}

          {isRunning && (
            <Button
              variant="destructive"
              onClick={handleCancel}
              className="w-full gap-2"
            >
              <XCircle className="size-4" />
              Cancel
            </Button>
          )}

          {/* Progress section */}
          {(isRunning || isDone) && (
            <div className="space-y-3">
              {/* Step progress bar */}
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-medium text-foreground">
                    {currentStep
                      ? TRANSFER_STEPS.find((s) => s.key === currentStep)
                          ?.label ?? currentStep
                      : "Starting..."}
                  </span>
                  <span className="text-[11px] text-muted-foreground">
                    {currentStepIndex}/{totalSteps}
                  </span>
                </div>
                <Progress value={progressPercent} />
              </div>

              {/* Step labels */}
              <div className="flex flex-wrap gap-1">
                {TRANSFER_STEPS.map((step, i) => {
                  const stepIdx = getStepIndex(step.key);
                  let variant: "default" | "secondary" | "outline" = "outline";
                  if (stepIdx < currentStepIndex) variant = "default";
                  else if (stepIdx === currentStepIndex && isRunning)
                    variant = "secondary";

                  return (
                    <Badge
                      key={step.key}
                      variant={variant}
                      className={`text-[10px] ${
                        stepIdx === currentStepIndex && isRunning
                          ? "animate-pulse"
                          : ""
                      }`}
                    >
                      {step.label}
                    </Badge>
                  );
                })}
              </div>

              {/* Log viewer */}
              {logs.length > 0 && (
                <div className="space-y-1.5">
                  <Label className="text-xs text-muted-foreground">Log</Label>
                  <div className="max-h-48 overflow-y-auto rounded-md border border-border bg-muted/30 p-2">
                    {logs.map((line, i) => (
                      <div
                        key={i}
                        className={`font-mono text-[11px] leading-relaxed ${
                          line.startsWith("Error")
                            ? "text-destructive"
                            : "text-muted-foreground"
                        }`}
                      >
                        {line}
                      </div>
                    ))}
                    <div ref={logEndRef} />
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Completion state */}
          {status === "success" && (
            <div className="flex items-start gap-2 rounded-md border border-emerald-500/30 bg-emerald-500/5 px-3 py-2.5">
              <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
              <div className="space-y-1">
                <p className="text-xs font-medium text-foreground">
                  Transfer completed successfully.
                </p>
                <p className="text-[11px] text-muted-foreground">
                  Your document has been migrated to the{" "}
                  {selectedTemplate?.name ?? "selected"} template. Review the
                  output and compile to verify.
                </p>
              </div>
            </div>
          )}

          {/* Error state */}
          {status === "failed" && error && (
            <div className="space-y-2">
              <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2.5">
                <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
                <p className="text-xs text-destructive">{error}</p>
              </div>
              <Button
                variant="outline"
                onClick={handleRetry}
                className="w-full gap-2"
              >
                <RefreshCw className="size-4" />
                Retry
              </Button>
            </div>
          )}

          {/* Cancelled state */}
          {status === "cancelled" && (
            <div className="flex items-start gap-2 rounded-md border border-border bg-muted/30 px-3 py-2.5">
              <XCircle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <p className="text-xs text-muted-foreground">
                Transfer was cancelled.
              </p>
            </div>
          )}

          {/* Reset button for completed/failed/cancelled states */}
          {isDone && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleReset}
              className="w-full text-xs"
            >
              Start New Transfer
            </Button>
          )}

          {/* No project warning */}
          {!projectRoot && (
            <div className="flex items-center gap-2 rounded-md border border-border bg-muted/50 px-3 py-2.5">
              <AlertCircle className="size-4 shrink-0 text-muted-foreground" />
              <p className="text-xs text-muted-foreground">
                Open a project to use template transfer.
              </p>
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
