#!/usr/bin/env bash
# Block every git command that writes, for every agent AND the orchestrator.
#
# This file must live here, at .claude/hooks/, because that is the path
# .claude/settings.json points at. An earlier copy sat in .claude/agents/
# beside a settings.json that Claude Code never reads, so nothing was ever
# registered and three commits went through unopposed.
#
# It covers BOTH the Bash and PowerShell tools. Covering only Bash is the
# same defect by another name: on Windows the orchestrator reaches for
# PowerShell by default, so a Bash-only matcher blocks the tool that is not
# being used.
#
# It is NOT a security boundary. It cannot see inside a script file, and it
# cannot see `python -c "subprocess.run(['git','push'])"`. The .githooks
# pre-commit hook is the backstop for those, because git runs it whoever
# calls — enable it once with:  git config core.hooksPath .githooks

input=$(cat)

tool=$(printf '%s' "$input" | sed -n 's/.*"tool_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
case "$tool" in
  Bash|PowerShell) ;;
  *) exit 0 ;;
esac

cmd=$(printf '%s' "$input" | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\(.*\)".*/\1/p')

# git/gh, optionally preceded by anything, optionally carrying flags such as
# -C <path> or -c user.name=x, then a subcommand that writes.
if printf '%s' "$cmd" | grep -Eq '(^|[;&|`(]|[[:space:]])(git|gh)([[:space:]]+-[^[:space:]]+([[:space:]]+[^-][^[:space:]]*)?)*[[:space:]]+(commit|add|push|stash|reset|rebase|merge|tag|checkout|switch|restore|cherry-pick|revert|am|apply|clean|mv|rm|pr|release|gist)\b'; then
  cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"No AI commits on this project — this includes the orchestrator, not only subagents. Git write operations are blocked by policy; see CLAUDE.md. Report that a commit is needed, say exactly what should go in it, and stop. The owner writes every commit."}}
JSON
  exit 0
fi
exit 0
