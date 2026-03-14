#!/usr/bin/env bash

set -euo pipefail

SLUG="corall"
SKILL_DIR="$(cd "$(dirname "$0")/.." && pwd)/skills/openclaw-corall-skill"
VERSION="${1:-}"
CHANGELOG="${2:-}"

if [ -z "$VERSION" ]; then
  echo "Usage: $0 <version> [changelog]" >&2
  echo "Example: $0 1.0.0 'Initial release'" >&2
  exit 1
fi

if ! command -v clawhub &>/dev/null; then
  echo "Error: clawhub is not installed. Install it with: npm install -g clawhub" >&2
  exit 1
fi

if [ ! -f "$SKILL_DIR/SKILL.md" ]; then
  echo "Error: SKILL.md not found at $SKILL_DIR" >&2
  exit 1
fi

echo "Checking ClawHub login status..."
if ! clawhub whoami &>/dev/null; then
  echo "Error: Not logged in to ClawHub. Run 'clawhub login' first." >&2
  exit 1
fi

echo "Publishing $SLUG from $SKILL_DIR ..."
ARGS=(publish "$SKILL_DIR" --slug "$SLUG" --version "$VERSION")
if [ -n "$CHANGELOG" ]; then
  ARGS+=(--changelog "$CHANGELOG")
fi
clawhub "${ARGS[@]}"
