#!/usr/bin/env node
/**
 * Neex - Install Script
 *
 * Downloads the correct binary for the current platform/arch
 * Supports: darwin-arm64, darwin-x64, linux-x64, linux-arm64, win32-x64
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

const PLATFORMS = {
  'darwin-arm64': '@neexjs/darwin-arm64',
  'darwin-x64': '@neexjs/darwin-x64',
  'linux-x64': '@neexjs/linux-x64',
  'linux-arm64': '@neexjs/linux-arm64',
  'win32-x64': '@neexjs/win32-x64',
};

function getPlatformPackage() {
  const platform = os.platform();
  const arch = os.arch();
  const key = `${platform}-${arch}`;

  const pkg = PLATFORMS[key];
  if (!pkg) {
    console.error(`❌ Unsupported platform: ${key}`);
    console.error(`   Supported: ${Object.keys(PLATFORMS).join(', ')}`);
    process.exit(1);
  }

  return pkg;
}

function findBinary() {
  const pkg = getPlatformPackage();

  // Try to find the platform-specific package
  const possiblePaths = [
    // npm installs
    path.join(__dirname, 'node_modules', pkg, 'bin', 'neex'),
    path.join(__dirname, '..', pkg, 'bin', 'neex'),
    // pnpm installs
    path.join(__dirname, '..', '..', pkg, 'bin', 'neex'),
    path.join(__dirname, '..', '..', '..', pkg, 'bin', 'neex'),
  ];

  for (const binPath of possiblePaths) {
    const execPath = process.platform === 'win32' ? `${binPath}.exe` : binPath;
    if (fs.existsSync(execPath)) {
      return execPath;
    }
  }

  return null;
}

function copyBinary() {
  const sourcePath = findBinary();

  if (!sourcePath) {
    console.log('⚠️ Binary not found in optional dependencies');
    console.log('   This is normal for development. Build from source:');
    console.log('   cargo build --release -p neex-cli');
    return;
  }

  const targetPath = path.join(__dirname, 'bin', 'neex');
  const targetDir = path.dirname(targetPath);

  // Ensure bin directory exists
  if (!fs.existsSync(targetDir)) {
    fs.mkdirSync(targetDir, { recursive: true });
  }

  // Copy binary
  fs.copyFileSync(sourcePath, targetPath);

  // Make executable
  if (process.platform !== 'win32') {
    fs.chmodSync(targetPath, 0o755);
  }

  console.log('✅ Neex installed');
}

// Run
try {
  copyBinary();
} catch (err) {
  console.error('Install failed:', err.message);
  process.exit(1);
}
