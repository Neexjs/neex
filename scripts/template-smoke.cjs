#!/usr/bin/env node
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { createRequire } = require('node:module');
const repo = path.resolve(__dirname, '..');
const spawn = createRequire(path.join(repo, 'npm/create-neex/package.json'))('cross-spawn');
const binary = path.resolve(process.argv[3] || path.join(repo, 'target/release', process.platform === 'win32' ? 'neex.exe' : 'neex'));
const requested = process.argv[2];
const templates = requested ? [requested] : ['next-express', 'next-hono'];
for (const template of templates) assert.ok(['next-express', 'next-hono'].includes(template));

function run(command, args, cwd) {
  const result = spawn.sync(command, args, {
    cwd, encoding: 'utf8', timeout: 300000,
    env: { ...process.env, NEXT_TELEMETRY_DISABLED: '1', NEEX_REMOTE_CACHE_WRITE: 'never' },
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

function availablePort() {
  return new Promise(resolve => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolve(port)); });
  });
}

async function service(command, args, cwd, check) {
  const port = await availablePort();
  const child = spawn(command, args(port), {
    cwd, env: { ...process.env, PORT: String(port), NEXT_TELEMETRY_DISABLED: '1' }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  let logs = '';
  let error;
  const closed = new Promise(resolve => child.once('close', resolve));
  child.once('error', value => { error = value; });
  child.stdout.on('data', bytes => { logs += bytes; });
  child.stderr.on('data', bytes => { logs += bytes; });
  try {
    const deadline = Date.now() + 30000;
    let result;
    while (Date.now() < deadline) {
      if (error) throw error;
      assert.equal(child.exitCode, null, logs);
      try {
        const response = await fetch(`http://127.0.0.1:${port}${check.path}`, { signal: AbortSignal.timeout(1000) });
        if (response.ok) { result = await response.text(); break; }
      } catch {}
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.ok(result, `Service never became healthy\n${logs}`);
    check.verify(result);
  } finally {
    child.kill('SIGTERM');
    const timer = setTimeout(() => child.kill('SIGKILL'), 3000);
    try { await closed; } finally { clearTimeout(timer); }
  }
}

async function main() {
  assert.ok(fs.existsSync(binary), `Build CLI first: ${binary}`);
  for (const template of templates) {
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'neex-template-'));
    try {
      console.log(`${template}: creating project and installing dependencies`);
      run(process.execPath, [path.join(repo, 'npm/create-neex/dist/index.js'), 'smoke', '--template', template, '--no-git'], temp);
      const root = path.join(temp, 'smoke');
      for (const task of ['build', 'lint', 'typecheck']) {
        run(binary, [task, '--all', '--concurrency', '2', '--summarize'], root);
        console.log(`${template}: ${task} passed`);
      }
      const api = path.join(root, 'apps/api');
      const [runtime, ...startArgs] = JSON.parse(fs.readFileSync(path.join(api, 'package.json'))).scripts.start.split(' ');
      await service(runtime === 'node' ? process.execPath : runtime,
        () => startArgs, api,
        { path: '/health', verify: text => assert.equal(JSON.parse(text).status, 'ok') });
      const web = path.join(root, 'apps/web');
      await service(process.execPath,
        port => [path.join(web, 'node_modules/next/dist/bin/next'), 'start', '--port', String(port)], web,
        { path: '/', verify: text => assert.match(text, /Welcome to/) });
      // A production output directory may be deleted or contain old chunks.
      // Cache replay must restore the build and clean only declared outputs.
      fs.rmSync(path.join(web, '.next'), { recursive: true });
      fs.rmSync(path.join(api, 'dist'), { recursive: true });
      run(binary, ['build', '--all', '--summarize'], root);
      const runs = path.join(root, '.neex/runs');
      const last = JSON.parse(fs.readFileSync(path.join(runs, fs.readdirSync(runs).sort().at(-1))));
      assert.ok(last.tasks.some(task => task.id === '@smoke/web#build' && task.status === 'cache-hit'), 'Next build must restore from cache');
      assert.ok(last.tasks.some(task => task.id === '@smoke/api#build' && task.status === 'cache-hit'), 'API build must restore from cache');
      assert.ok(fs.existsSync(path.join(web, '.next/BUILD_ID')));
      assert.ok(fs.existsSync(path.join(api, 'dist/index.js')));
      console.log(`${template}: production HTTP and cache restore passed`);
    } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
