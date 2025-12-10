#!/bin/bash
# 🎨 Neex Code Formatter

set -e
echo "🎨 Formatting Neex codebase..."

cd crates
cargo fmt --all
cd ..

echo "✅ Formatting complete!"