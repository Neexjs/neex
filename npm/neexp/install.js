#!/usr/bin/env node
/**
 * Neexp - Install Script
 *
 * Downloads the correct binary for the current platform/arch
 * Supports: darwin-arm64, darwin-x64, linux-x64, linux-arm64, win32-x64
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

const PLATFORMS = {
  'darwin-arm64': '@neexpjs/darwin-arm64',
  'darwin-x64': '@neexpjs/darwin-x64',
  'linux-x64': '@neexpjs/linux-x64',
  'linux-arm64': '@neexpjs/linux-arm64',
  'win32-x64': '@neexpjs/win32-x64',
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
    path.join(__dirname, 'node_modules', pkg, 'bin', 'neexp'),
    path.join(__dirname, '..', pkg, 'bin', 'neexp'),
    // pnpm installs
    path.join(__dirname, '..', '..', pkg, 'bin', 'neexp'),
    path.join(__dirname, '..', '..', '..', pkg, 'bin', 'neexp'),
  ];

  try {
    const resolvedPath = require.resolve(`${pkg}/bin/neexp`);
    if (resolvedPath) {
      possiblePaths.unshift(resolvedPath);
    }
  } catch (e) {
    // Ignore require.resolve errors
  }

  for (const binPath of possiblePaths) {
    const execPath = process.platform === 'win32' && !binPath.endsWith('.exe') ? `${binPath}.exe` : binPath;
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
    console.log('   cargo build --release -p neexp-cli');
    return;
  }

  const targetPath = path.join(__dirname, 'bin', process.platform === 'win32' ? 'neexp.exe' : 'neexp');
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

  console.log('✅ Neexp installed');
}

// Run
try {
  copyBinary();
} catch (err) {
  console.error('Install failed:', err.message);
  process.exit(1);
}
