#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

log() {
    echo -e "${GREEN}[VERIFY]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

cleanup() {
    echo -e "${RED}[VERIFY]${NC} Creating cleanup... Killing child processes..."
    # Attempt to kill all child processes of the current script
    pkill -P $$ || true
}
trap cleanup EXIT INT TERM

PROJECT_ROOT=$(pwd)
CLI_DIR="$PROJECT_ROOT/packages/cli"
TEST_DIR="$PROJECT_ROOT/neex-verification-workspace"
TEST_PROJECT_NAME="scaffold-test"

# 1. Build CLI
log "Building CLI..."
cd "$CLI_DIR"
npm run build || error "CLI build failed"
cd "$PROJECT_ROOT" # Return to project root

log "Building Core..."
cd "$PROJECT_ROOT/packages/core"
bun install || error "bun install for core failed"
bun run build || error "Core build failed"
log "Building Neexa..."
cd "$PROJECT_ROOT/packages/neexa"
bun install || error "bun install for neexa failed"
bun run build || error "Neexa build failed"
cd "$PROJECT_ROOT" # Return to project root

# 2. Clean previous test
if [ -d "$TEST_DIR" ]; then
    log "Cleaning up previous test workspace..."
    rm -rf "$TEST_DIR"
fi
mkdir -p "$TEST_DIR"

# 3. Simulate CLI Execution
log "Generating test project..."
cd "$TEST_DIR"

log "Running create-neex non-interactively..."
# Using flags to bypass prompts: --type express-next for Monorepo, --bun for package manager
node "$CLI_DIR/dist/index.js" "$TEST_PROJECT_NAME" --type express-next --bun --debug

cd "$TEST_PROJECT_NAME"

log "Linking local 'neex' and 'neexa' for verification..."
# Replace "neex" and "neexa" with local paths in ROOT
if [[ "$OSTYPE" == "darwin"* ]]; then
  # Root package.json uses "latest"
  sed -i '' 's#"neex": "latest"#"neex": "file:../../packages/core"#' package.json
  sed -i '' 's#"neex": "\^0.7.45"#"neex": "file:../../packages/core"#' package.json
  sed -i '' 's#"neexa": "\^0.1.0"#"neexa": "file:../../packages/neexa"#' package.json
  sed -i '' 's#"neexa": "latest"#"neexa": "file:../../packages/neexa"#' package.json
  
  # Replace in ALL apps/*/package.json files
  find apps -name "package.json" -maxdepth 2 -exec sed -i '' 's#"neex": "\^[0-9.]*"#"neex": "file:../../../../packages/core"#' {} +
  find apps -name "package.json" -maxdepth 2 -exec sed -i '' 's#"neexa": "\^[0-9.]*"#"neexa": "file:../../../../packages/neexa"#' {} +
else
  sed -i 's#"neex": "latest"#"neex": "file:../../packages/core"#' package.json
  sed -i 's#"neex": "\^0.7.45"#"neex": "file:../../packages/core"#' package.json
  sed -i 's#"neexa": "\^0.1.0"#"neexa": "file:../../packages/neexa"#' package.json
  sed -i 's#"neexa": "latest"#"neexa": "file:../../packages/neexa"#' package.json
  
  # Replace in ALL apps/*/package.json files
  find apps -name "package.json" -maxdepth 2 -exec sed -i 's#"neex": "\^[0-9.]*"#"neex": "file:../../../../packages/core"#' {} +
  find apps -name "package.json" -maxdepth 2 -exec sed -i 's#"neexa": "\^[0-9.]*"#"neexa": "file:../../../../packages/neexa"#' {} +
fi

# 4. Static Checks
log "Validating Generated Structure..."

# Check neex.json
if grep -q "inputs" neex.json; then
    log "neex.json has 'inputs' configuration (Good)"
else
    error "neex.json missing 'inputs' configuration"
fi

if grep -q "hashingStrategy.*auto" neex.json; then
    log "neex.json uses 'hashingStrategy: auto' for Native Zig (Good)"
else
    error "neex.json missing 'hashingStrategy: auto'"
fi

# Check workspace dependency
if grep -q "workspace:\*" apps/web/package.json; then
    log "Web app (apps/web) uses workspace:* protocol (Good)"
else
    error "Web app package.json missing workspace:* dependency. Content: $(cat apps/web/package.json)"
fi

# 5. Build Verification
log "Installing dependencies (bun install)..."
bun install || error "bun install failed"
echo "[VERIFY] Debugging Neex installation..."
ls -l node_modules/neex/dist/src

echo "[VERIFY] Running neex build..."
# neex is in devDependencies, so we should be able to run it.
bun run build || error "bun run build failed"

log "SUCCESS! Neex Ecosystem Verified."
log "Project is at: $TEST_DIR/$TEST_PROJECT_NAME"
