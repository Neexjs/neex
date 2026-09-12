#!/usr/bin/env node
/**
 * Neex - Install Script
 *
 * Copies the prebuilt binary from the matching optional platform package
 * Supports: darwin-arm64, darwin-x64, linux-x64, win32-x64
 *
 * Never fails the install: if no binary can be copied, bin/neex.js keeps
 * looking for it at runtime and reports a clear error there.
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

const PLATFORMS = {
  'darwin-arm64': '@neexjs/darwin-arm64',
  'darwin-x64': '@neexjs/darwin-x64',
  'linux-x64': '@neexjs/linux-x64',
  'win32-x64': '@neexjs/win32-x64',
};

function getPlatformPackage() {
  const key = `${os.platform()}-${os.arch()}`;
  const pkg = PLATFORMS[key];

  if (!pkg) {
    console.warn(`⚠️ Neex has no prebuilt binary for ${key}`);
    console.warn(`   Supported: ${Object.keys(PLATFORMS).join(', ')}`);
    console.warn('   Build from source: cargo build --release -p neex-cli');
    return null;
  }

  return pkg;
}

function findBinary(pkg) {
  // Try to find the platform-specific package
  const possiblePaths = [
    // npm installs
    path.join(__dirname, 'node_modules', pkg, 'bin', 'neex'),
    path.join(__dirname, '..', pkg, 'bin', 'neex'),
    // pnpm installs
    path.join(__dirname, '..', '..', pkg, 'bin', 'neex'),
    path.join(__dirname, '..', '..', '..', pkg, 'bin', 'neex'),
  ];

  try {
    const resolvedPath = require.resolve(`${pkg}/bin/neex`);
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
  const pkg = getPlatformPackage();
  if (!pkg) {
    return;
  }

  const sourcePath = findBinary(pkg);

  if (!sourcePath) {
    console.log('⚠️ Binary not found in optional dependencies');
    console.log('   This is normal for development. Build from source:');
    console.log('   cargo build --release -p neex-cli');
    return;
  }

  const targetPath = path.join(__dirname, 'bin', process.platform === 'win32' ? 'neex.exe' : 'neex');
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
  console.warn('⚠️ Neex postinstall could not copy the binary:', err.message);
}
