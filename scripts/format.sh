#!/bin/bash
# 🎨 Neexp Code Formatter

set -e
echo "🎨 Formatting Neexp codebase..."

cd crates
cargo fmt --all
cd ..

echo "✅ Formatting complete!"