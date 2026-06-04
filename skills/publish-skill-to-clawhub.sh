#!/usr/bin/env bash

set -euo pipefail

PACKAGE_DIR="$(cd "$(dirname "$0")/.." && pwd)/skills/corall"
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

if [ ! -f "$PACKAGE_DIR/SKILL.md" ]; then
  echo "Error: SKILL.md not found at $PACKAGE_DIR" >&2
  exit 1
fi

if [ ! -f "$PACKAGE_DIR/.claude-plugin/plugin.json" ]; then
  echo "Error: .claude-plugin/plugin.json not found at $PACKAGE_DIR" >&2
  exit 1
fi

echo "Checking ClawHub login status..."
if ! clawhub whoami &>/dev/null; then
  echo "Error: Not logged in to ClawHub. Run 'clawhub login' first." >&2
  exit 1
fi

echo "Publishing corall package from $PACKAGE_DIR ..."
ARGS=(package publish "$PACKAGE_DIR" --version "$VERSION")
if [ -n "$CHANGELOG" ]; then
  echo "Note: package publish does not accept the skill-publish changelog flag; changelog content is not passed to ClawHub." >&2
fi
clawhub "${ARGS[@]}"
