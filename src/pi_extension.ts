/**
 * ecotokens extension for pi - token savings via pre/post tool interception
 * Install: ecotokens install --target pi
 * Do not edit manually — regenerate with: ecotokens install --target pi
 */
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { execSync, spawnSync } from "child_process";

// ─── helpers ────────────────────────────────────────────────────────────────

function ecotokensAvailable(): boolean {
  try {
    execSync("ecotokens --version", { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function extractText(content: unknown[]): string {
  return (content as Array<{ type: string; text?: string }>)
    .filter((c) => c?.type === "text")
    .map((c) => c.text ?? "")
    .join("\n");
}

/**
 * Appelle ecotokens hook-post avec le payload PostHookInput.
 * Retourne additionalContext (output filtré + outline) ou null si passthrough.
 *
 * Format attendu par src/hook/post_handler.rs::PostHookInput :
 *   { tool_name, tool_input, tool_response: { output }, cwd }
 */
function callHookPost(
  toolName: string,
  toolInput: unknown,
  output: string,
  cwd: string
): string | null {
  const payload = JSON.stringify({
    tool_name: toolName,
    tool_input: toolInput,
    tool_response: { output },
    cwd,
  });

  const result = spawnSync("ecotokens", ["hook-post"], {
    input: payload,
    encoding: "utf-8",
    timeout: 10_000,
  });

  if (result.status !== 0 || !result.stdout) return null;

  try {
    const parsed = JSON.parse(result.stdout) as {
      hookSpecificOutput?: { additionalContext?: string };
    };
    return parsed?.hookSpecificOutput?.additionalContext ?? null;
  } catch {
    return null;
  }
}

// Mapping noms d'outils Pi (lowercase) → noms capitalisés pour handle_post_input()
// src/hook/post_handler.rs:66 route sur "Read" | "Grep" | "Glob"
const PI_TO_CLAUDE_TOOL: Record<string, string> = {
  read: "Read",
  grep: "Grep",
  find: "Glob",
  ls: "Glob",
};

// Taille minimale pour déclencher le filtrage (évite les spawns inutiles)
const MIN_OUTPUT_CHARS = 200;

// ─── extension ──────────────────────────────────────────────────────────────

export default function (pi: ExtensionAPI) {
  if (!ecotokensAvailable()) {
    console.warn("[ecotokens] binary not found in PATH, extension disabled");
    return;
  }

  // ── 1. Bash pre-execution : équivalent PreToolUse rewrite ────────────────
  //
  // Mutation de event.input.command avant exécution (extensions.md:570).
  // Le rendering natif Pi est conservé (pas de tool override).
  // ecotokens filter exécute la commande, filtre stdout, enregistre les métriques.
  pi.on("tool_call", async (event, ctx) => {
    if (event.toolName !== "bash") return;
    const input = event.input as { command?: string };
    if (!input.command) return;
    input.command = `ecotokens filter --cwd ${JSON.stringify(ctx.cwd)} -- bash -c ${JSON.stringify(input.command)}`;
  });

  // ── 2. Outils natifs post-execution : équivalent PostToolUse ─────────────
  //
  // tool_result (extensions.md:625) : retour partiel remplace content.
  // Cible : read, grep, find, ls — mêmes handlers que Claude PostToolUse.
  pi.on("tool_result", async (event, ctx) => {
    const claudeName = PI_TO_CLAUDE_TOOL[event.toolName];
    if (!claudeName) return;

    const rawOutput = extractText(event.content as unknown[]);
    if (rawOutput.length < MIN_OUTPUT_CHARS) return;

    const filtered = callHookPost(claudeName, event.input, rawOutput, ctx.cwd);
    if (!filtered || filtered === rawOutput) return;

    return { content: [{ type: "text", text: filtered }] };
  });

  // ── 3. Session lifecycle : auto-watch ────────────────────────────────────
  pi.on("session_start", async (event, _ctx) => {
    if (event.reason === "startup") {
      spawnSync("ecotokens", ["session-start"], { stdio: "ignore" });
    }
  });

  pi.on("session_shutdown", async (_event, _ctx) => {
    spawnSync("ecotokens", ["session-end"], { stdio: "ignore" });
  });

  // ── 4. Commandes slash ────────────────────────────────────────────────────
  pi.registerCommand("gain", {
    description: "Show ecotokens token savings report",
    handler: async (args, ctx) => {
      const period = args?.trim() || "all";
      try {
        const result = spawnSync("ecotokens", ["gain", "--period", period], { encoding: "utf-8" });
        const out = (result.stdout as string) || "";
        ctx.ui.notify(out.trim().slice(0, 500), "info");
      } catch (e: unknown) {
        ctx.ui.notify(`ecotokens gain failed: ${(e as Error).message}`, "error");
      }
    },
  });

  pi.registerCommand("eco-search", {
    description: "Search codebase via ecotokens index",
    handler: async (args, ctx) => {
      if (!args?.trim()) {
        ctx.ui.notify("Usage: /eco-search <query>", "info");
        return;
      }
      try {
        const result = spawnSync("ecotokens", ["search", args.trim()], { encoding: "utf-8" });
        const out = (result.stdout as string) || "";
        const raw = out.trim();
        const display = raw.length > 800 ? raw.slice(0, 800) + " …(truncated)" : raw;
        ctx.ui.notify(display, "info");
      } catch (e: unknown) {
        ctx.ui.notify(`eco-search failed: ${(e as Error).message}`, "error");
      }
    },
  });
}
