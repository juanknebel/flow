#!/usr/bin/env bash
# Adds priority: MEDIUM frontmatter to card .md files missing it
set -euo pipefail

BOARD_DIR="${1:-.board}"

if [ ! -d "$BOARD_DIR" ]; then
    echo "Directory not found: $BOARD_DIR"
    exit 1
fi

find "$BOARD_DIR" -name "*.md" | while read -r file; do
    if head -1 "$file" | grep -q "^---$"; then
        echo "SKIP: $file"
    else
        content=$(cat "$file")
        printf -- '---\npriority: MEDIUM\n---\n%s' "$content" > "$file"
        echo "MIGRATED: $file"
    fi
done
