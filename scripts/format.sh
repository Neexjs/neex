#!/bin/bash
# 🎨 Neex Code Formatter

set -e
echo "🎨 Formatting Neex codebase..."

cd "$(dirname "$0")/.."
cargo fmt --all

echo "✅ Formatting complete!"
