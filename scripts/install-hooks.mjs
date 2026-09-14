#!/usr/bin/env node
/**
 * Install (or remove) Clawd's hook entries in Claude Code's settings.
 *
 * Clawd observes Claude Code through hooks, and hooks are configured in a file
 * the user owns and has their own entries in. So this merges rather than
 * writes: it finds or creates the matcher group for each event, drops any
 * entry pointing at Clawd's URL, and appends a fresh one. Identity is the URL,
 * which makes it idempotent without adding a marker key Claude Code's schema
 * validator has never heard of.
 *
 *   npm run hooks:install
 *   npm run hooks:uninstall
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import process from "node:process";

/** Matches `DEFAULT_PORT` in src-tauri/src/hook.rs. */
const DEFAULT_PORT = 8787;

/**
 * The events Clawd listens for.
 *
 * `SessionStart` is registered with no matcher and filtered in the translator
 * instead: a session resuming from a compaction must not reset the pet, and
 * that rule is worth a unit test rather than a matcher regex.
 *
 * Tool events use `"*"` because the pet shows every tool, not a chosen few.
 */
const TOOL_EVENTS = [
  "PreToolUse",
  "PostToolUse",
  "PostToolUseFailure",
  "PermissionRequest",
];
const PLAIN_EVENTS = [
  "SessionStart",
  "UserPromptSubmit",
  "Notification",
  "Stop",
  "StopFailure",
  "PreCompact",
  "PostCompact",
  "SessionEnd",
];

/**
 * Short, so a wedged Clawd can never hold up a turn. The listener answers
 * before it parses, so this is headroom rather than a budget.
 */
const TIMEOUT_SECONDS = 5;

const settingsPath = join(homedir(), ".claude", "settings.json");

function die(message) {
  console.error(`\n  ${message}\n`);
  process.exit(1);
}

/**
 * The port Clawd actually bound, when it has run and recorded one.
 *
 * The default may already have been taken, in which case Clawd scanned upward
 * — pointing the config at a port nothing is listening on would look exactly
 * like a broken integration.
 */
function livePort() {
  const dirs =
    process.platform === "win32"
      ? [process.env.APPDATA && join(process.env.APPDATA, "com.clawd.desktop")]
      : process.platform === "darwin"
        ? [join(homedir(), "Library", "Application Support", "com.clawd.desktop")]
        : [join(homedir(), ".config", "com.clawd.desktop")];

  for (const dir of dirs.filter(Boolean)) {
    const file = join(dir, "hook-port.json");
    if (!existsSync(file)) continue;
    try {
      const { port } = JSON.parse(readFileSync(file, "utf8"));
      if (Number.isInteger(port) && port > 0 && port < 65536) return port;
    } catch {
      // A corrupt hint file is not worth failing over; fall through.
    }
  }
  return DEFAULT_PORT;
}

/**
 * Read the settings file.
 *
 * Parses strictly and refuses to continue on a syntax error rather than
 * salvaging what it can. A round-trip through JSON.parse/stringify silently
 * discards comments and trailing commas, and people do put both in this file;
 * destroying someone's config to install a desk toy is not a trade worth
 * making.
 */
function readSettings() {
  if (!existsSync(settingsPath)) return {};
  const text = readFileSync(settingsPath, "utf8");
  if (text.trim() === "") return {};
  try {
    const parsed = JSON.parse(text);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      die(`${settingsPath} is not a JSON object. Nothing was written.`);
    }
    return parsed;
  } catch (error) {
    die(
      `Could not parse ${settingsPath}:\n  ${error.message}\n\n` +
        `  Nothing was written. Fix the file by hand, or paste the snippet\n` +
        `  from the README's "Connecting it to Claude Code" section instead.`,
    );
  }
}

function backup(text) {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const path = `${settingsPath}.clawd-backup-${stamp}`;
  writeFileSync(path, text, "utf8");
  return path;
}

const isClawd = (entry, url) => entry && entry.type === "http" && entry.url === url;

/** Drop every Clawd entry, and every group left empty by doing so. */
function removeClawd(hooks, url) {
  let removed = 0;
  for (const event of Object.keys(hooks)) {
    if (!Array.isArray(hooks[event])) continue;
    for (const group of hooks[event]) {
      if (!group || !Array.isArray(group.hooks)) continue;
      const before = group.hooks.length;
      group.hooks = group.hooks.filter((entry) => !isClawd(entry, url));
      removed += before - group.hooks.length;
    }
    // A group we emptied was ours; one that was already empty is not our
    // business to tidy.
    hooks[event] = hooks[event].filter(
      (group) => !group || !Array.isArray(group.hooks) || group.hooks.length > 0,
    );
    if (hooks[event].length === 0) delete hooks[event];
  }
  return removed;
}

function addClawd(hooks, url) {
  const entry = { type: "http", url, timeout: TIMEOUT_SECONDS };

  for (const [event, matcher] of [
    ...TOOL_EVENTS.map((e) => [e, "*"]),
    ...PLAIN_EVENTS.map((e) => [e, undefined]),
  ]) {
    if (!Array.isArray(hooks[event])) hooks[event] = [];
    // Join the group whose matcher already matches, rather than adding a
    // second one beside an identical matcher.
    let group = hooks[event].find((g) => g && g.matcher === matcher);
    if (!group) {
      group = matcher === undefined ? { hooks: [] } : { matcher, hooks: [] };
      hooks[event].push(group);
    }
    if (!Array.isArray(group.hooks)) group.hooks = [];
    group.hooks.push({ ...entry });
  }
}

function main() {
  const uninstall = process.argv.includes("--uninstall");
  const port = livePort();
  const url = `http://127.0.0.1:${port}/hook`;

  const settings = readSettings();
  const existed = existsSync(settingsPath);
  const original = existed ? readFileSync(settingsPath, "utf8") : "";

  if (!settings.hooks || typeof settings.hooks !== "object") {
    if (uninstall) {
      console.log("\n  No hooks configured. Nothing to remove.\n");
      return;
    }
    settings.hooks = {};
  }

  // Always clear first, so installing twice does not leave two copies.
  const removed = removeClawd(settings.hooks, url);
  if (!uninstall) addClawd(settings.hooks, url);
  if (Object.keys(settings.hooks).length === 0) delete settings.hooks;

  if (uninstall && removed === 0) {
    console.log(`\n  No Clawd hooks found for ${url}. Nothing to remove.\n`);
    return;
  }

  const saved = existed && original.trim() !== "" ? backup(original) : null;
  mkdirSync(dirname(settingsPath), { recursive: true });
  writeFileSync(settingsPath, `${JSON.stringify(settings, null, 2)}\n`, "utf8");

  const events = TOOL_EVENTS.length + PLAIN_EVENTS.length;
  console.log(
    uninstall
      ? `\n  Removed ${removed} Clawd hook ${removed === 1 ? "entry" : "entries"}.`
      : `\n  Installed ${events} Clawd hooks pointing at ${url}.`,
  );
  console.log(`  Settings: ${settingsPath}`);
  if (saved) console.log(`  Backup:   ${saved}`);
  if (!uninstall && port === DEFAULT_PORT) {
    console.log(
      `\n  Clawd has not recorded a port yet, so this uses the default.\n` +
        `  If it ever binds elsewhere (the port was taken), re-run this.`,
    );
  }
  console.log(
    uninstall
      ? "\n  Claude Code picks this up on its next session.\n"
      : "\n  Start Clawd, then start a Claude Code session anywhere.\n",
  );
}

main();
