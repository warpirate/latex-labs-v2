import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Users,
  Play,
  Square,
  Link,
  Copy,
  CheckCircle2,
  AlertCircle,
  Wifi,
  WifiOff,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useDocumentStore } from "@/stores/document-store";

// ── Types ──

interface CollabSession {
  id: string;
  project_dir: string;
  host_name: string;
  port: number;
  active: boolean;
  connected_users: string[];
}

type ConnectionStatus = "disconnected" | "hosting" | "connecting";

// ── Component ──

export function CollabPanel() {
  const projectRoot = useDocumentStore((s) => s.projectRoot);

  // Host state
  const [displayName, setDisplayName] = useState("");
  const [port, setPort] = useState("9090");
  const [session, setSession] = useState<CollabSession | null>(null);
  const [inviteLink, setInviteLink] = useState("");
  const [copied, setCopied] = useState(false);
  const [status, setStatus] = useState<ConnectionStatus>("disconnected");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Join state
  const [joinLink, setJoinLink] = useState("");

  // Check existing session on mount
  useEffect(() => {
    (async () => {
      try {
        const existing = await invoke<CollabSession | null>("get_collab_status");
        if (existing) {
          setSession(existing);
          setStatus("hosting");
          setDisplayName(existing.host_name);
          setPort(String(existing.port));
          const link = await invoke<string>("generate_collab_invite", {
            sessionId: existing.id,
          });
          setInviteLink(link);
        }
      } catch {
        // No active session
      }
    })();
  }, []);

  const handleStartHosting = useCallback(async () => {
    if (!displayName.trim()) {
      setError("Display name is required");
      return;
    }
    if (!projectRoot) {
      setError("No project is open");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const portNum = parseInt(port, 10) || 9090;
      const newSession = await invoke<CollabSession>("start_collab_session", {
        projectDir: projectRoot,
        hostName: displayName.trim(),
        port: portNum,
      });
      setSession(newSession);
      setStatus("hosting");

      const link = await invoke<string>("generate_collab_invite", {
        sessionId: newSession.id,
      });
      setInviteLink(link);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [displayName, port, projectRoot]);

  const handleStopHosting = useCallback(async () => {
    if (!session) return;
    setLoading(true);
    try {
      await invoke("stop_collab_session", { sessionId: session.id });
      setSession(null);
      setStatus("disconnected");
      setInviteLink("");
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [session]);

  const handleCopyInvite = useCallback(async () => {
    if (!inviteLink) return;
    try {
      await navigator.clipboard.writeText(inviteLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback: select text
    }
  }, [inviteLink]);

  // ── Render ──

  return (
    <ScrollArea className="h-full">
      <div className="space-y-6 p-4">
        {/* Header */}
        <div className="flex items-center gap-2">
          <Users className="size-5 text-[var(--muted-foreground)]" />
          <h2 className="text-sm font-semibold">Collaboration</h2>
          <div className="ml-auto">
            {status === "hosting" ? (
              <Badge variant="default" className="gap-1 text-xs">
                <Wifi className="size-3" />
                Hosting
              </Badge>
            ) : (
              <Badge variant="secondary" className="gap-1 text-xs">
                <WifiOff className="size-3" />
                Offline
              </Badge>
            )}
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="flex items-start gap-2 rounded-md border border-red-500/30 bg-red-500/10 p-3">
            <AlertCircle className="mt-0.5 size-4 shrink-0 text-red-400" />
            <p className="text-xs text-red-400">{error}</p>
          </div>
        )}

        {/* Host Session Section */}
        <div className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--card)] p-4">
          <h3 className="text-xs font-medium text-[var(--muted-foreground)] uppercase tracking-wider">
            Host Session
          </h3>

          {status !== "hosting" ? (
            <>
              <div className="space-y-1.5">
                <Label htmlFor="collab-name" className="text-xs">
                  Display Name
                </Label>
                <Input
                  id="collab-name"
                  placeholder="Your name"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  className="h-8 text-sm"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="collab-port" className="text-xs">
                  Port
                </Label>
                <Input
                  id="collab-port"
                  placeholder="9090"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                  className="h-8 text-sm"
                  type="number"
                />
              </div>
              <Button
                onClick={handleStartHosting}
                disabled={loading || !displayName.trim()}
                className="w-full gap-2"
                size="sm"
              >
                {loading ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Play className="size-4" />
                )}
                Start Hosting
              </Button>
            </>
          ) : (
            <>
              <div className="flex items-center gap-2 text-sm">
                <CheckCircle2 className="size-4 text-green-400" />
                <span>
                  Hosting as <strong>{session?.host_name}</strong> on port{" "}
                  <strong>{session?.port}</strong>
                </span>
              </div>

              {/* Invite link */}
              {inviteLink && (
                <div className="space-y-1.5">
                  <Label className="text-xs">Invite Link</Label>
                  <div className="flex items-center gap-1">
                    <Input
                      readOnly
                      value={inviteLink}
                      className="h-8 text-xs font-mono"
                    />
                    <Button
                      variant="outline"
                      size="icon"
                      className="size-8 shrink-0"
                      onClick={handleCopyInvite}
                      title="Copy invite link"
                    >
                      {copied ? (
                        <CheckCircle2 className="size-3.5 text-green-400" />
                      ) : (
                        <Copy className="size-3.5" />
                      )}
                    </Button>
                  </div>
                </div>
              )}

              {/* Connected users */}
              <div className="space-y-1.5">
                <Label className="text-xs">
                  Connected Users ({session?.connected_users.length ?? 0})
                </Label>
                {session?.connected_users.length === 0 ? (
                  <p className="text-xs text-[var(--muted-foreground)]">
                    No users connected yet
                  </p>
                ) : (
                  <div className="space-y-1">
                    {session?.connected_users.map((user, i) => (
                      <div
                        key={i}
                        className="flex items-center gap-2 rounded-md bg-[var(--accent)] px-2 py-1 text-xs"
                      >
                        <Users className="size-3" />
                        {user}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <Button
                onClick={handleStopHosting}
                disabled={loading}
                variant="destructive"
                className="w-full gap-2"
                size="sm"
              >
                {loading ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Square className="size-4" />
                )}
                Stop Hosting
              </Button>
            </>
          )}
        </div>

        {/* Join Session Section */}
        <div className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--card)] p-4">
          <h3 className="text-xs font-medium text-[var(--muted-foreground)] uppercase tracking-wider">
            Join Session
          </h3>
          <div className="space-y-1.5">
            <Label htmlFor="join-link" className="text-xs">
              Invite Link
            </Label>
            <Input
              id="join-link"
              placeholder="latexlabs://collab?host=..."
              value={joinLink}
              onChange={(e) => setJoinLink(e.target.value)}
              className="h-8 text-xs font-mono"
            />
          </div>
          <Button
            variant="outline"
            disabled={!joinLink.trim()}
            className="w-full gap-2"
            size="sm"
          >
            <Link className="size-4" />
            Join
          </Button>
          <p className="text-[10px] text-[var(--muted-foreground)]">
            Joining sessions is a preview feature and will be fully functional in
            a future release.
          </p>
        </div>

        {/* Info note */}
        <p className="text-center text-[10px] text-[var(--muted-foreground)] leading-relaxed">
          Full real-time collaboration coming soon. Currently supports session
          management.
        </p>
      </div>
    </ScrollArea>
  );
}
