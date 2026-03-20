import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import {
  FileTextIcon,
  SearchIcon,
  ScanIcon,
  BarChartIcon,
  ArrowRightLeftIcon,
  GitCompareArrowsIcon,
  UsersIcon,
} from "lucide-react";
import { Sidebar } from "./sidebar";
import { LatexEditor } from "./editor/latex-editor";
import { PdfPreview } from "./preview/pdf-preview";
import { VisionPanel } from "@/components/panels/vision-panel";
import { ChartPanel } from "@/components/panels/chart-panel";
import { TransferPanel } from "@/components/panels/transfer-panel";
import { CollabPanel } from "@/components/panels/collab-panel";
import { ArxivPanel } from "@/components/panels/arxiv-panel";
import { useDocumentStore } from "@/stores/document-store";
import { cn } from "@/lib/utils";

function PanelTabButton({
  icon: Icon,
  label,
  active,
  onClick,
}: {
  icon: any;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
        active
          ? "bg-[var(--primary)] text-white"
          : "text-[var(--muted-foreground)] hover:bg-[var(--accent)] hover:text-[var(--foreground)]"
      )}
    >
      <Icon size={13} />
      {label}
    </button>
  );
}

export function WorkspaceLayout() {
  const initialized = useDocumentStore((s) => s.initialized);
  const rightPanelView = useDocumentStore((s) => s.rightPanelView);
  const setRightPanelView = useDocumentStore((s) => s.setRightPanelView);

  if (!initialized) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-muted-foreground">Loading project...</div>
      </div>
    );
  }

  return (
    <PanelGroup direction="horizontal" className="h-full">
      <Panel defaultSize={15} minSize={10} maxSize={25}>
        <Sidebar />
      </Panel>

      <PanelResizeHandle className="w-px bg-border transition-colors hover:bg-ring" />

      <Panel defaultSize={42.5} minSize={25}>
        <LatexEditor />
      </Panel>

      <PanelResizeHandle className="w-px bg-border transition-colors hover:bg-ring" />

      <Panel defaultSize={42.5} minSize={25}>
        <div className="flex h-full flex-col">
          {/* Panel tab bar */}
          <div className="flex items-center gap-1 border-b border-[var(--border)] bg-[var(--card)] px-2 py-1">
            <PanelTabButton
              icon={FileTextIcon}
              label="PDF"
              active={rightPanelView === "pdf"}
              onClick={() => setRightPanelView("pdf")}
            />
            <PanelTabButton
              icon={SearchIcon}
              label="ArXiv"
              active={rightPanelView === "arxiv"}
              onClick={() => setRightPanelView("arxiv")}
            />
            <PanelTabButton
              icon={ScanIcon}
              label="Vision"
              active={rightPanelView === "vision"}
              onClick={() => setRightPanelView("vision")}
            />
            <PanelTabButton
              icon={BarChartIcon}
              label="Chart"
              active={rightPanelView === "chart"}
              onClick={() => setRightPanelView("chart")}
            />
            <PanelTabButton
              icon={ArrowRightLeftIcon}
              label="Transfer"
              active={rightPanelView === "transfer"}
              onClick={() => setRightPanelView("transfer")}
            />
            <PanelTabButton
              icon={UsersIcon}
              label="Collab"
              active={rightPanelView === "collab"}
              onClick={() => setRightPanelView("collab")}
            />
            <PanelTabButton
              icon={GitCompareArrowsIcon}
              label="Review"
              active={rightPanelView === "review"}
              onClick={() => setRightPanelView("review")}
            />
          </div>
          {/* Panel content */}
          <div className="flex-1 overflow-hidden">
            {rightPanelView === "pdf" && <PdfPreview />}
            {rightPanelView === "arxiv" && <ArxivPanel />}
            {rightPanelView === "vision" && <VisionPanel />}
            {rightPanelView === "chart" && <ChartPanel />}
            {rightPanelView === "transfer" && <TransferPanel />}
            {rightPanelView === "collab" && <CollabPanel />}
            {rightPanelView === "review" && (
              <div className="flex h-full items-center justify-center text-muted-foreground">
                Review panel coming soon
              </div>
            )}
          </div>
        </div>
      </Panel>
    </PanelGroup>
  );
}
