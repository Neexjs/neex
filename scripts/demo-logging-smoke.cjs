#!/usr/bin/env node
// Exercise the actual demo API with production-only dependencies. No pretty
// transport may be required in staging/production, and query secrets stay out of logs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { createRequire } = require('node:module');
const repo = path.resolve(__dirname, '..');
const spawn = createRequire(path.join(repo, 'npm/create-neex/package.json'))('cross-spawn');

async function main() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'neex-demo-logging-'));
  try {
    const api = path.join(repo, 'demo/apps/api');
    const pkg = JSON.parse(fs.readFileSync(path.join(api, 'package.json')));
    const dependencies = Object.fromEntries(Object.entries(pkg.dependencies).filter(([, version]) => !version.startsWith('workspace:')));
    fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ private: true, type: 'module', dependencies }));
    fs.cpSync(path.join(api, 'src'), path.join(root, 'src'), { recursive: true });
    const install = spawn.sync('npm', ['install', '--omit=dev', '--no-audit', '--no-fund'], { cwd: root, encoding: 'utf8', timeout: 120000 });
    assert.ifError(install.error);
    assert.equal(install.status, 0, install.stdout + install.stderr);
    assert.equal(fs.existsSync(path.join(root, 'node_modules/pino-pretty')), false);
    for (const environment of ['staging', 'production']) {
      const port = await new Promise((resolve, reject) => {
        const server = net.createServer();
        server.once('error', reject);
        server.listen(0, '127.0.0.1', () => { const value = server.address().port; server.close(() => resolve(value)); });
      });
      const child = spawn('bun', ['src/server.ts'], {
        cwd: root, env: { ...process.env, NODE_ENV: environment, LOG_LEVEL: 'info', PORT: String(port) },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      const closed = new Promise(resolve => child.once('close', resolve));
      let error;
      let logs = '';
      child.once('error', value => { error = value; });
      child.stdout.on('data', value => { logs += value; });
      child.stderr.on('data', value => { logs += value; });
      try {
        const deadline = Date.now() + 10000;
        let healthy = false;
        while (Date.now() < deadline) {
          if (error) throw error;
          assert.equal(child.exitCode, null, logs);
          try {
            const result = await fetch(`http://127.0.0.1:${port}/api/health?token=secret-smoke-value`, { signal: AbortSignal.timeout(1000) });
            if (result.ok) { assert.equal((await result.json()).success, true); healthy = true; break; }
          } catch {}
          await new Promise(resolve => setTimeout(resolve, 50));
        }
        assert.ok(healthy, logs);
        await new Promise(resolve => setTimeout(resolve, 100));
        assert.match(logs, /"url":"\/api\/health"/);
        assert.doesNotMatch(logs, /secret-smoke-value/);
        console.log(`Demo logging passed (${environment}, no dev dependencies, query redacted)`);
      } finally {
        child.kill('SIGTERM');
        const timer = setTimeout(() => child.kill('SIGKILL'), 3000);
        try { await closed; } finally { clearTimeout(timer); }
      }
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
