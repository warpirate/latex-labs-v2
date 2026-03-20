import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useDocumentStore } from "./document-store";
import { createLogger } from "@/lib/debug/logger";

const log = createLogger("agent");

// ── Types ──

export interface LlmConfig {
  endpoint: string;
  apiKey: string;
  model: string;
  visionModel?: string;
}

export interface ToolCallInfo {
  name: string;
  args: Record<string, unknown>;
  result?: string;
}

export interface AgentResult {
  reply: string;
  suggestion?: string;
  patches: Array<{
    filePath: string;
    oldContent: string;
    newContent: string;
  }>;
}

interface AgentStore {
  // LLM Configuration
  llmConfig: LlmConfig;
  setLlmConfig: (config: Partial<LlmConfig>) => void;

  // Agent state
  isRunning: boolean;
  progress: string[];
  toolCalls: ToolCallInfo[];
  result: AgentResult | null;
  error: string | null;

  // Actions
  runAgent: (
    task: string,
    prompt: string,
    selection?: string,
    content?: string,
  ) => Promise<void>;
  clearResult: () => void;
}

// ── LocalStorage persistence ──

const STORAGE_KEY = "latexlabs-llm-config";

const DEFAULT_CONFIG: LlmConfig = {
  endpoint: "https://api.openai.com/v1/chat/completions",
  apiKey: "",
  model: "gpt-4o-mini",
};

function loadConfig(): LlmConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return { ...DEFAULT_CONFIG, ...parsed };
    }
  } catch {
    // Corrupted data — fall back to defaults
  }
  return { ...DEFAULT_CONFIG };
}

function saveConfig(config: LlmConfig) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    log.error("Failed to persist LLM config to localStorage");
  }
}

// ── Event listeners ──

let unlisteners: UnlistenFn[] = [];
let listenersInitialized = false;

async function initEventListeners(store: typeof useAgentStore) {
  if (listenersInitialized) return;
  listenersInitialized = true;

  try {
    const u1 = await listen<string>("agent-progress", (event) => {
      log.debug("agent-progress", { payload: event.payload });
      store.setState((s) => ({
        progress: [...s.progress, event.payload],
      }));
    });

    const u2 = await listen<ToolCallInfo>("agent-tool-call", (event) => {
      log.debug("agent-tool-call", { name: event.payload.name });
      store.setState((s) => ({
        toolCalls: [...s.toolCalls, event.payload],
      }));
    });

    const u3 = await listen<AgentResult | { error: string }>(
      "agent-complete",
      (event) => {
        const payload = event.payload;
        if ("error" in payload) {
          log.error("Agent completed with error", { error: payload.error });
          store.setState({
            isRunning: false,
            error: payload.error,
          });
        } else {
          log.info("Agent completed successfully");
          store.setState({
            isRunning: false,
            result: payload,
          });
        }
      },
    );

    unlisteners = [u1, u2, u3];
  } catch (err) {
    log.error("Failed to initialize agent event listeners", {
      error: String(err),
    });
    listenersInitialized = false;
  }
}

// ── Store ──

export const useAgentStore = create<AgentStore>()((set, get) => {
  // Initialize event listeners lazily
  // We do this in a microtask to avoid calling listen during module evaluation
  queueMicrotask(() => {
    initEventListeners(useAgentStore);
  });

  return {
    llmConfig: loadConfig(),

    isRunning: false,
    progress: [],
    toolCalls: [],
    result: null,
    error: null,

    setLlmConfig: (partial) => {
      const current = get().llmConfig;
      const updated = { ...current, ...partial };
      saveConfig(updated);
      set({ llmConfig: updated });
    },

    runAgent: async (task, prompt, selection?, content?) => {
      const { llmConfig, isRunning } = get();
      if (isRunning) {
        log.warn("Agent is already running, ignoring duplicate call");
        return;
      }

      const projectDir = useDocumentStore.getState().projectRoot;
      if (!projectDir) {
        set({ error: "No project open" });
        return;
      }

      // Ensure listeners are ready
      await initEventListeners(useAgentStore);

      // Reset state
      set({
        isRunning: true,
        progress: [],
        toolCalls: [],
        result: null,
        error: null,
      });

      log.info("Starting agent", { task, model: llmConfig.model });

      try {
        await invoke("run_agent", {
          task,
          prompt,
          selection: selection ?? null,
          content: content ?? null,
          projectDir,
          endpoint: llmConfig.endpoint,
          apiKey: llmConfig.apiKey,
          model: llmConfig.model,
        });
        // Completion is handled via the "agent-complete" event listener
      } catch (err: any) {
        const msg = err?.message || String(err);
        log.error("run_agent invoke failed", { error: msg });
        set({
          isRunning: false,
          error: msg,
        });
      }
    },

    clearResult: () => {
      set({
        progress: [],
        toolCalls: [],
        result: null,
        error: null,
      });
    },
  };
});
