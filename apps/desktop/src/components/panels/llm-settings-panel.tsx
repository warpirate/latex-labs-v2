import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { useAgentStore } from "@/stores/agent-store";
import {
  SettingsIcon,
  EyeIcon,
  EyeOffIcon,
  Loader2Icon,
  CheckCircleIcon,
  XCircleIcon,
  WifiIcon,
} from "lucide-react";
import { createLogger } from "@/lib/debug/logger";

const log = createLogger("llm-settings");

type ConnectionStatus = "idle" | "testing" | "success" | "error";

export function LlmSettingsPanel() {
  const llmConfig = useAgentStore((s) => s.llmConfig);
  const setLlmConfig = useAgentStore((s) => s.setLlmConfig);

  const [endpoint, setEndpoint] = React.useState(llmConfig.endpoint);
  const [apiKey, setApiKey] = React.useState(llmConfig.apiKey);
  const [model, setModel] = React.useState(llmConfig.model);
  const [visionModel, setVisionModel] = React.useState(
    llmConfig.visionModel ?? "",
  );
  const [showKey, setShowKey] = React.useState(false);
  const [connectionStatus, setConnectionStatus] =
    React.useState<ConnectionStatus>("idle");
  const [connectionMessage, setConnectionMessage] = React.useState("");
  const [open, setOpen] = React.useState(false);

  // Sync local state when store changes (e.g. dialog reopened)
  React.useEffect(() => {
    setEndpoint(llmConfig.endpoint);
    setApiKey(llmConfig.apiKey);
    setModel(llmConfig.model);
    setVisionModel(llmConfig.visionModel ?? "");
  }, [llmConfig, open]);

  const hasChanges =
    endpoint !== llmConfig.endpoint ||
    apiKey !== llmConfig.apiKey ||
    model !== llmConfig.model ||
    visionModel !== (llmConfig.visionModel ?? "");

  const isConfigValid = endpoint.trim() !== "" && model.trim() !== "";

  const handleSave = React.useCallback(() => {
    setLlmConfig({
      endpoint: endpoint.trim(),
      apiKey,
      model: model.trim(),
      visionModel: visionModel.trim() || undefined,
    });
    log.info("LLM config saved");
    setOpen(false);
  }, [endpoint, apiKey, model, visionModel, setLlmConfig]);

  const handleTestConnection = React.useCallback(async () => {
    if (!endpoint.trim() || !model.trim()) {
      setConnectionStatus("error");
      setConnectionMessage("Endpoint and model are required.");
      return;
    }

    setConnectionStatus("testing");
    setConnectionMessage("");

    try {
      const response = await fetch(endpoint.trim(), {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(apiKey ? { Authorization: `Bearer ${apiKey}` } : {}),
        },
        body: JSON.stringify({
          model: model.trim(),
          messages: [
            { role: "user", content: "Say 'ok' and nothing else." },
          ],
          max_tokens: 5,
        }),
      });

      if (response.ok) {
        const data = await response.json();
        const reply =
          data?.choices?.[0]?.message?.content ?? "(no response body)";
        setConnectionStatus("success");
        setConnectionMessage(`Connected. Model replied: "${reply.slice(0, 50)}"`);
        log.info("Connection test succeeded");
      } else {
        const text = await response.text().catch(() => "");
        setConnectionStatus("error");
        setConnectionMessage(
          `HTTP ${response.status}: ${text.slice(0, 200) || response.statusText}`,
        );
        log.error("Connection test failed", { status: response.status });
      }
    } catch (err: any) {
      setConnectionStatus("error");
      setConnectionMessage(err?.message || String(err));
      log.error("Connection test error", { error: String(err) });
    }
  }, [endpoint, apiKey, model]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="icon-sm" title="LLM Settings">
          <SettingsIcon className="size-4" />
        </Button>
      </DialogTrigger>

      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>LLM Configuration</DialogTitle>
          <DialogDescription>
            Configure the endpoint, API key, and model for the AI agent.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          {/* Endpoint */}
          <div className="space-y-1.5">
            <Label htmlFor="llm-endpoint">Endpoint URL</Label>
            <Input
              id="llm-endpoint"
              type="url"
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
              placeholder="https://api.openai.com/v1/chat/completions"
              className="h-8 text-sm"
            />
          </div>

          {/* API Key */}
          <div className="space-y-1.5">
            <Label htmlFor="llm-api-key">API Key</Label>
            <div className="relative">
              <Input
                id="llm-api-key"
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="sk-..."
                className="h-8 pr-9 text-sm"
              />
              <button
                type="button"
                onClick={() => setShowKey((v) => !v)}
                className="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                tabIndex={-1}
              >
                {showKey ? (
                  <EyeOffIcon className="size-4" />
                ) : (
                  <EyeIcon className="size-4" />
                )}
              </button>
            </div>
          </div>

          {/* Model */}
          <div className="space-y-1.5">
            <Label htmlFor="llm-model">Model</Label>
            <Input
              id="llm-model"
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="gpt-4o-mini"
              className="h-8 text-sm"
            />
          </div>

          {/* Vision Model (optional) */}
          <div className="space-y-1.5">
            <Label htmlFor="llm-vision-model">
              Vision Model{" "}
              <span className="font-normal text-muted-foreground">(optional)</span>
            </Label>
            <Input
              id="llm-vision-model"
              type="text"
              value={visionModel}
              onChange={(e) => setVisionModel(e.target.value)}
              placeholder="gpt-4o"
              className="h-8 text-sm"
            />
          </div>

          {/* Test Connection */}
          <div className="space-y-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleTestConnection}
              disabled={connectionStatus === "testing" || !isConfigValid}
              className="w-full"
            >
              {connectionStatus === "testing" ? (
                <Loader2Icon className="size-4 animate-spin" />
              ) : (
                <WifiIcon className="size-4" />
              )}
              Test Connection
            </Button>

            {/* Status indicator */}
            {connectionStatus !== "idle" && connectionStatus !== "testing" && (
              <div
                className={`flex items-start gap-2 rounded-md border p-2 text-xs ${
                  connectionStatus === "success"
                    ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                    : "border-destructive/30 bg-destructive/10 text-destructive"
                }`}
              >
                {connectionStatus === "success" ? (
                  <CheckCircleIcon className="mt-0.5 size-3.5 shrink-0" />
                ) : (
                  <XCircleIcon className="mt-0.5 size-3.5 shrink-0" />
                )}
                <span className="break-all">{connectionMessage}</span>
              </div>
            )}
          </div>

          {/* Config validity indicator */}
          <div className="flex items-center gap-2 text-xs">
            <div
              className={`size-2 rounded-full ${
                isConfigValid && apiKey
                  ? "bg-emerald-500"
                  : isConfigValid
                    ? "bg-amber-500"
                    : "bg-destructive"
              }`}
            />
            <span className="text-muted-foreground">
              {isConfigValid && apiKey
                ? "Configuration looks complete"
                : isConfigValid
                  ? "API key is empty (may be required)"
                  : "Endpoint and model are required"}
            </span>
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setOpen(false)}
          >
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!hasChanges || !isConfigValid}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
