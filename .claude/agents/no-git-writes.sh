#!/usr/bin/env bash
# Block every git command that writes, for every agent and the orchestrator.
#
# The deny rules in settings.json catch the obvious form. This catches the
# variants they miss: `git -C <path> commit`, `cd x && git commit`,
# `git -c user.name=x commit`, and anything chained behind && or ;.
#
# It is NOT a security boundary. It cannot see inside a script file, and it
# cannot see `python -c "subprocess.run(['git','push'])"`. The .githooks
# pre-commit hook is the backstop for those, because git runs it whoever calls.

input=$(cat)
tool=$(printf '%s' "$input" | sed -n 's/.*"tool_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
[ "$tool" = "Bash" ] || exit 0
cmd=$(printf '%s' "$input" | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\(.*\)".*/\1/p')

# git/gh followed by any flags, then a writing subcommand
if printf '%s' "$cmd" | grep -Eq '(^|[;&|[:space:]])(git|gh)([[:space:]]+-[^[:space:]]+([[:space:]]+[^-][^[:space:]]*)?)*[[:space:]]+(commit|add|push|stash|reset|rebase|merge|tag|checkout|switch|restore|cherry-pick|revert|am|apply|clean|pr|release)\b'; then
  cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"No AI commits. Git write operations are blocked by project policy — see CLAUDE.md. Report that a commit is needed and stop; the owner writes every commit."}}
JSON
  exit 0
fi
exit 0
