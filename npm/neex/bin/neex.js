#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const os = require('os');

const PLATFORMS = {
  'darwin-arm64': '@neexjs/darwin-arm64',
  'darwin-x64': '@neexjs/darwin-x64',
  'linux-x64': '@neexjs/linux-x64',
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
    // After the postinstall copy (which is chmod'ed), before the guessed paths
    const resolvedPath = require.resolve(`${pkg}/bin/${process.platform === 'win32' ? 'neex.exe' : 'neex'}`);
    if (resolvedPath) {
      possiblePaths.splice(1, 0, resolvedPath);
    }
  } catch (e) {
    // Ignore require.resolve errors
  }

  for (const binPath of possiblePaths) {
    const execPath = process.platform === 'win32' && !binPath.endsWith('.exe') ? `${binPath}.exe` : binPath;
    if (fs.existsSync(execPath)) {
      ensureExecutable(execPath);
      return execPath;
    }
  }

  return null;
}

// Published platform tarballs may lose the executable bit
function ensureExecutable(binPath) {
  if (process.platform === 'win32') {
    return;
  }
  try {
    fs.accessSync(binPath, fs.constants.X_OK);
  } catch (e) {
    try {
      fs.chmodSync(binPath, 0o755);
    } catch (chmodErr) {
      // Read-only install location; spawn will report EACCES
    }
  }
}

function run() {
  const binaryPath = findBinary();

  if (!binaryPath) {
    console.error('❌ Could not find Neex binary for your platform.');
    console.error('   Please ensure you have installed it correctly.');
    console.error('   If you are building from source, make sure to compile neex-cli first.');
    process.exit(1);
  }

  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
  });

  // Keep the launcher alive until the native CLI has cleaned up its tasks.
  // spawnSync blocks JS signal handlers and can orphan the native process.
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(signal, () => { child.kill(signal); });
  }

  child.once('error', error => {
    console.error(`❌ Failed to run ${binaryPath}: ${error.message}`);
    process.exit(1);
  });
  child.once('exit', (code, signal) => {
    if (signal) {
      process.removeAllListeners(signal);
      process.exitCode = 1;
      process.kill(process.pid, signal);
    } else {
      process.exit(code ?? 1);
    }
  });
}

run();
