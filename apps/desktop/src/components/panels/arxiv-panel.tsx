import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useDocumentStore } from "@/stores/document-store";
import {
  SearchIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  BookOpenIcon,
  DownloadIcon,
  Loader2Icon,
  FileTextIcon,
} from "lucide-react";
import { createLogger } from "@/lib/debug/logger";

const log = createLogger("arxiv");

// ── Types ──

interface ArxivResult {
  id: string;
  title: string;
  authors: string[];
  summary: string;
  published: string;
  updated: string;
  pdfUrl?: string;
  arxivUrl?: string;
}

// ── Component ──

export function ArxivPanel() {
  const [query, setQuery] = React.useState("");
  const [maxResults, setMaxResults] = React.useState("10");
  const [results, setResults] = React.useState<ArxivResult[]>([]);
  const [isSearching, setIsSearching] = React.useState(false);
  const [searchError, setSearchError] = React.useState<string | null>(null);
  const [expandedIds, setExpandedIds] = React.useState<Set<string>>(new Set());
  const [addingBibtex, setAddingBibtex] = React.useState<Set<string>>(
    new Set(),
  );
  const [downloadingSource, setDownloadingSource] = React.useState<Set<string>>(
    new Set(),
  );

  const projectRoot = useDocumentStore((s) => s.projectRoot);

  const handleSearch = React.useCallback(async () => {
    const trimmed = query.trim();
    if (!trimmed) return;

    setIsSearching(true);
    setSearchError(null);
    setResults([]);

    try {
      log.info("Searching arXiv", { query: trimmed, maxResults });
      const data = await invoke<ArxivResult[]>("arxiv_search", {
        query: trimmed,
        maxResults: parseInt(maxResults, 10),
      });
      setResults(data);
      log.info(`Found ${data.length} results`);
    } catch (err: any) {
      const msg = err?.message || String(err);
      log.error("ArXiv search failed", { error: msg });
      setSearchError(msg);
    } finally {
      setIsSearching(false);
    }
  }, [query, maxResults]);

  const handleKeyDown = React.useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleSearch();
      }
    },
    [handleSearch],
  );

  const toggleExpanded = React.useCallback((id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const handleAddBibtex = React.useCallback(
    async (arxivId: string) => {
      if (!projectRoot) return;

      setAddingBibtex((prev) => new Set(prev).add(arxivId));
      try {
        const bibtex = await invoke<string>("arxiv_bibtex", { arxivId });

        // Find or create references.bib
        const docStore = useDocumentStore.getState();
        const bibFile = docStore.files.find(
          (f) => f.name === "references.bib" || f.name === "bibliography.bib",
        );

        if (bibFile) {
          const existing = bibFile.content ?? "";
          const newContent = existing.trimEnd() + "\n\n" + bibtex.trim() + "\n";
          docStore.updateFileContent(bibFile.id, newContent);
          log.info(`Appended BibTeX for ${arxivId} to ${bibFile.name}`);
        } else {
          // Create references.bib
          await docStore.createNewFile("references.bib", "tex");
          // After creation, update its content
          const newBibFile = docStore.files.find(
            (f) => f.name === "references.bib",
          );
          if (newBibFile) {
            docStore.updateFileContent(newBibFile.id, bibtex.trim() + "\n");
          }
          log.info(`Created references.bib with BibTeX for ${arxivId}`);
        }
      } catch (err: any) {
        log.error("Failed to add BibTeX", { error: String(err) });
      } finally {
        setAddingBibtex((prev) => {
          const next = new Set(prev);
          next.delete(arxivId);
          return next;
        });
      }
    },
    [projectRoot],
  );

  const handleDownloadSource = React.useCallback(
    async (arxivId: string) => {
      if (!projectRoot) return;

      setDownloadingSource((prev) => new Set(prev).add(arxivId));
      try {
        await invoke("arxiv_download_source", { arxivId, projectDir: projectRoot });
        log.info(`Downloaded source for ${arxivId}`);
        // Refresh file tree to show imported files
        await useDocumentStore.getState().refreshFiles();
      } catch (err: any) {
        log.error("Failed to download source", { error: String(err) });
      } finally {
        setDownloadingSource((prev) => {
          const next = new Set(prev);
          next.delete(arxivId);
          return next;
        });
      }
    },
    [projectRoot],
  );

  const extractYear = (dateStr: string) => {
    try {
      return new Date(dateStr).getFullYear().toString();
    } catch {
      return "";
    }
  };

  const truncateAuthors = (authors: string[], max = 3) => {
    if (authors.length <= max) return authors.join(", ");
    return authors.slice(0, max).join(", ") + ` et al.`;
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <BookOpenIcon className="size-4 text-muted-foreground" />
        <span className="font-medium text-sm">ArXiv Search</span>
      </div>

      {/* Search Controls */}
      <div className="space-y-2 border-b p-3">
        <div className="flex gap-2">
          <Input
            placeholder="Search papers..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            className="h-8 flex-1 text-sm"
          />
          <Button
            size="sm"
            onClick={handleSearch}
            disabled={isSearching || !query.trim()}
          >
            {isSearching ? (
              <Loader2Icon className="size-4 animate-spin" />
            ) : (
              <SearchIcon className="size-4" />
            )}
          </Button>
        </div>

        <div className="flex items-center gap-2">
          <span className="text-muted-foreground text-xs">Max results:</span>
          <Select value={maxResults} onValueChange={setMaxResults}>
            <SelectTrigger size="sm" className="h-7 w-20 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="5">5</SelectItem>
              <SelectItem value="10">10</SelectItem>
              <SelectItem value="20">20</SelectItem>
              <SelectItem value="50">50</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Results */}
      <ScrollArea className="flex-1">
        <div className="p-3">
          {/* Loading */}
          {isSearching && (
            <div className="flex flex-col items-center gap-2 py-8 text-muted-foreground">
              <Loader2Icon className="size-6 animate-spin" />
              <span className="text-sm">Searching arXiv...</span>
            </div>
          )}

          {/* Error */}
          {searchError && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 p-3 text-destructive text-sm">
              {searchError}
            </div>
          )}

          {/* Empty state */}
          {!isSearching && !searchError && results.length === 0 && (
            <div className="flex flex-col items-center gap-2 py-8 text-muted-foreground">
              <FileTextIcon className="size-8 opacity-50" />
              <span className="text-sm">
                Search for papers on arXiv to find references.
              </span>
            </div>
          )}

          {/* Result list */}
          {results.length > 0 && (
            <div className="space-y-3">
              {results.map((paper) => {
                const isExpanded = expandedIds.has(paper.id);
                const isBibtexLoading = addingBibtex.has(paper.id);
                const isSourceLoading = downloadingSource.has(paper.id);
                const year = extractYear(paper.published);

                return (
                  <div
                    key={paper.id}
                    className="rounded-md border bg-card p-3 text-card-foreground"
                  >
                    {/* Title */}
                    <a
                      href={
                        paper.arxivUrl ||
                        `https://arxiv.org/abs/${paper.id}`
                      }
                      target="_blank"
                      rel="noopener noreferrer"
                      className="font-medium text-sm leading-tight text-primary hover:underline"
                    >
                      {paper.title}
                    </a>

                    {/* Meta row */}
                    <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
                      <Badge variant="secondary" className="text-[10px]">
                        {paper.id}
                      </Badge>
                      {year && (
                        <Badge variant="outline" className="text-[10px]">
                          {year}
                        </Badge>
                      )}
                    </div>

                    {/* Authors */}
                    <p className="mt-1.5 text-muted-foreground text-xs leading-relaxed">
                      {truncateAuthors(paper.authors)}
                    </p>

                    {/* Abstract (collapsible) */}
                    <button
                      type="button"
                      onClick={() => toggleExpanded(paper.id)}
                      className="mt-1.5 flex items-center gap-1 text-muted-foreground text-xs hover:text-foreground"
                    >
                      {isExpanded ? (
                        <ChevronUpIcon className="size-3" />
                      ) : (
                        <ChevronDownIcon className="size-3" />
                      )}
                      {isExpanded ? "Hide abstract" : "Show abstract"}
                    </button>

                    {isExpanded && (
                      <p className="mt-1.5 text-muted-foreground text-xs leading-relaxed">
                        {paper.summary}
                      </p>
                    )}

                    {/* Actions */}
                    <div className="mt-2 flex gap-2">
                      <Button
                        variant="outline"
                        size="xs"
                        onClick={() => handleAddBibtex(paper.id)}
                        disabled={isBibtexLoading || !projectRoot}
                      >
                        {isBibtexLoading ? (
                          <Loader2Icon className="size-3 animate-spin" />
                        ) : (
                          <BookOpenIcon className="size-3" />
                        )}
                        Add BibTeX
                      </Button>
                      <Button
                        variant="outline"
                        size="xs"
                        onClick={() => handleDownloadSource(paper.id)}
                        disabled={isSourceLoading || !projectRoot}
                      >
                        {isSourceLoading ? (
                          <Loader2Icon className="size-3 animate-spin" />
                        ) : (
                          <DownloadIcon className="size-3" />
                        )}
                        Import Source
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
