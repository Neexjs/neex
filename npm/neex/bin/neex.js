#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');
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
    // Next to this script (if postinstall succeeded)
    path.join(__dirname, process.platform === 'win32' ? 'neex.exe' : 'neex'),
    // npm installs
    path.join(__dirname, '..', 'node_modules', pkg, 'bin', 'neex'),
    path.join(__dirname, '..', '..', pkg, 'bin', 'neex'),
    // pnpm installs
    path.join(__dirname, '..', '..', '..', pkg, 'bin', 'neex'),
    path.join(__dirname, '..', '..', '..', '..', pkg, 'bin', 'neex'),
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

function run() {
  const binaryPath = findBinary();

  if (!binaryPath) {
    console.error('❌ Could not find Neex binary for your platform.');
    console.error('   Please ensure you have installed it correctly.');
    console.error('   If you are building from source, make sure to compile neex-cli first.');
    process.exit(1);
  }

  const { status, signal } = spawnSync(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
  });

  if (signal) {
    process.exit(1);
  }

  process.exit(status !== null ? status : 1);
}

run();
