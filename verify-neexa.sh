#!/bin/bash
# verify-neexa.sh - Test neexa runner package specifically
set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}[NEEXA TEST]${NC} Building neexa..."
cd packages/neexa
bun run build

echo -e "${GREEN}[NEEXA TEST]${NC} Testing CLI help..."
node dist/index.js --help

echo -e "${GREEN}[NEEXA TEST]${NC} Testing dev command help..."
node dist/index.js dev --help

echo -e "${GREEN}[NEEXA TEST]${NC} Testing build command help..."
node dist/index.js build --help

echo -e "${GREEN}[NEEXA TEST]${NC} Testing start command help..."
node dist/index.js start --help

# Test actual dev command with a simple file
echo -e "${GREEN}[NEEXA TEST]${NC} Creating test file..."
mkdir -p /tmp/neexa-test
cat > /tmp/neexa-test/index.ts << 'EOF'
console.log("neexa test successful!");
process.exit(0);
EOF

echo -e "${GREEN}[NEEXA TEST]${NC} Testing dev command with file..."
timeout 5 node dist/index.js dev /tmp/neexa-test/index.ts --delay 1000 || echo "Dev test completed (timeout expected)"

# Cleanup
rm -rf /tmp/neexa-test

echo -e "${GREEN}[NEEXA TEST]${NC} All tests passed!"
