#!/usr/bin/env node
/**
 * Install (or remove) Clawd's hook entries in Claude Code's settings.
 *
 * Clawd observes Claude Code through hooks, and hooks are configured in a file
 * the user owns and has their own entries in. So this merges rather than
 * writes: it finds or creates the matcher group for each event, drops any
 * entry pointing at Clawd's loopback endpoint, and appends a fresh one.
 * Identity is that endpoint, which makes it idempotent across a port change
 * without adding a marker key Claude Code's schema validator has never heard
 * of.
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
 * `SessionStart` is the one event that cannot use an HTTP hook.
 *
 * Claude Code refuses them for `SessionStart` and `Setup` specifically, and it
 * does so silently — the entry is filtered out at match time with a debug
 * line, so an `http` entry here looks installed and simply never fires. It
 * gets a `command` hook posting the same body with curl instead, which ships
 * on Windows 10 1803+ and on macOS. Exec form, so no shell is involved.
 *
 * The response body is `{}`, which a command hook reads as valid hook output
 * expressing no opinion, so letting curl write it to stdout is harmless.
 */
const curlEntry = (url) => ({
  type: "command",
  command: "curl",
  args: [
    "-s",
    "-m",
    "2",
    "-X",
    "POST",
    "-H",
    "Content-Type: application/json",
    "--data-binary",
    "@-",
    url,
  ],
  timeout: TIMEOUT_SECONDS,
});

/**
 * The events Clawd listens for, minus `SessionStart`, which is added
 * separately because it needs the other transport.
 *
 * All of them register with no matcher and are filtered in the translator
 * instead — a session resuming from a compaction must not reset the pet, and
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

/**
 * Is this one of ours?
 *
 * Matched on the loopback `/hook` endpoint rather than on the exact URL,
 * because the port can change: install while 8787 is taken and Clawd binds
 * 8788, and an exact-URL match would then fail to find the entry it wrote,
 * leaving twelve dead hooks in the user's global settings with no way to
 * remove them but by hand. Covers both transports.
 */
function isClawd(entry) {
  if (!entry) return false;
  const target = entry.type === "http" ? entry.url : (entry.args ?? []).join(" ");
  return (
    typeof target === "string" &&
    /https?:\/\/(127\.0\.0\.1|localhost):\d+\/hook\b/.test(target)
  );
}

/** Drop every Clawd entry, and every group left empty by doing so. */
function removeClawd(hooks) {
  let removed = 0;
  for (const event of Object.keys(hooks)) {
    if (!Array.isArray(hooks[event])) continue;
    for (const group of hooks[event]) {
      if (!group || !Array.isArray(group.hooks)) continue;
      const before = group.hooks.length;
      group.hooks = group.hooks.filter((entry) => !isClawd(entry));
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
  const http = { type: "http", url, timeout: TIMEOUT_SECONDS };

  for (const [event, matcher] of [
    ...TOOL_EVENTS.map((e) => [e, "*"]),
    ...PLAIN_EVENTS.map((e) => [e, undefined]),
    ["SessionStart", undefined],
  ]) {
    const entry = event === "SessionStart" ? curlEntry(url) : http;
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
  const removed = removeClawd(settings.hooks);
  if (!uninstall) addClawd(settings.hooks, url);
  if (Object.keys(settings.hooks).length === 0) delete settings.hooks;

  if (uninstall && removed === 0) {
    console.log(`\n  No Clawd hooks found for ${url}. Nothing to remove.\n`);
    return;
  }

  const saved = existed && original.trim() !== "" ? backup(original) : null;
  mkdirSync(dirname(settingsPath), { recursive: true });
  writeFileSync(settingsPath, `${JSON.stringify(settings, null, 2)}\n`, "utf8");

  const events = TOOL_EVENTS.length + PLAIN_EVENTS.length + 1;
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
