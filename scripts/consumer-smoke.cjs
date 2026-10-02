#!/usr/bin/env node
// Test exactly what a consumer installs: packed launcher, platform binary,
// and scaffolder. No npm publishing and no reliance on a globally installed CLI.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createRequire } = require('node:module');
const spawn = createRequire(path.resolve(__dirname, '../npm/create-neex/package.json'))('cross-spawn');

const repo = path.resolve(__dirname, '..');
const binName = process.platform === 'win32' ? 'neex.exe' : 'neex';
const binary = path.resolve(process.argv[2] || path.join(repo, 'target/release', binName));
const platform = `${process.platform}-${process.arch}`;
const packages = {
  'darwin-arm64': '@neexjs/darwin-arm64', 'darwin-x64': '@neexjs/darwin-x64',
  'linux-x64': '@neexjs/linux-x64', 'win32-x64': '@neexjs/win32-x64',
};
assert.ok(packages[platform], `No package mapping for ${platform}`);
assert.ok(fs.existsSync(binary), `Build the CLI first: ${binary}`);

function run(command, args, cwd, env = {}) {
  const result = spawn.sync(command, args, {
    cwd, encoding: 'utf8', env: { ...process.env, ...env }, timeout: 240000,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

function pack(source, destination) {
  const result = JSON.parse(run('npm', ['pack', '--json', '--pack-destination', destination], source));
  return path.join(destination, result[0].filename);
}

function summary(root) {
  const dir = path.join(root, '.neex/runs');
  const file = fs.readdirSync(dir).sort().at(-1);
  return JSON.parse(fs.readFileSync(path.join(dir, file)));
}

async function cancellation(launcher, root, signal) {
  if (process.platform === 'win32') return; // Native Job Object cancellation is tested in Rust.
  const fixture = path.join(root, 'heartbeat.cjs');
  const beat = path.join(root, 'heartbeat');
  fs.rmSync(beat, { force: true });
  fs.writeFileSync(fixture, `const fs = require('node:fs');
if (!process.env.NEEX_SMOKE_LEAF) {
  require('node:child_process').spawn(process.execPath, [__filename], {
    env: { ...process.env, NEEX_SMOKE_LEAF: '1' }, stdio: 'inherit'
  });
} else {
  setInterval(() => fs.appendFileSync(${JSON.stringify(beat)}, 'beat\\n'), 50);
}
setTimeout(() => process.exit(0), 10000);
`);
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'consumer', scripts: { dev: 'node heartbeat.cjs' } }));
  const child = spawn(process.execPath, [launcher, 'dev'], { cwd: root, stdio: 'ignore' });
  const exit = new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
  try {
    const deadline = Date.now() + 5000;
    while (!fs.existsSync(beat) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 25));
    assert.ok(fs.existsSync(beat), 'nested task did not start');
    child.kill(signal);
    let timer;
    try {
      await Promise.race([exit, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`launcher ignored ${signal}`)), 5000);
      })]);
    } finally { clearTimeout(timer); }
    await new Promise(resolve => setTimeout(resolve, 150));
    const size = fs.statSync(beat).size;
    await new Promise(resolve => setTimeout(resolve, 200));
    assert.equal(fs.statSync(beat).size, size, `task grandchild survived launcher ${signal}`);
  } finally { child.kill('SIGTERM'); }
}

async function main() {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'neex-consumer-'));
  try {
    const wrapper = JSON.parse(fs.readFileSync(path.join(repo, 'npm/neex/package.json')));
    const platformDir = path.join(temp, 'platform');
    fs.mkdirSync(path.join(platformDir, 'bin'), { recursive: true });
    fs.writeFileSync(path.join(platformDir, 'package.json'), JSON.stringify({
      name: packages[platform], version: wrapper.optionalDependencies[packages[platform]],
      os: [process.platform], cpu: [process.arch], files: ['bin'],
    }));
    fs.copyFileSync(binary, path.join(platformDir, 'bin', binName));
    const platformTarball = pack(platformDir, temp);
    const launcherTarball = pack(path.join(repo, 'npm/neex'), temp);
    for (const ignoreScripts of [false, true]) {
      const root = path.join(temp, ignoreScripts ? 'without-postinstall' : 'with-postinstall');
      fs.mkdirSync(root);
      fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'consumer', private: true }));
      const args = ['install', '--omit=optional', '--no-audit', '--no-fund', platformTarball, launcherTarball];
      if (ignoreScripts) args.push('--ignore-scripts');
      run('npm', args, root);
      const launcher = path.join(root, 'node_modules/neex/bin/neex.js');
      const copied = path.join(root, 'node_modules/neex/bin', binName);
      assert.equal(fs.existsSync(copied), !ignoreScripts, 'postinstall must copy the native binary');
      const platformBinary = path.join(root, 'node_modules', packages[platform], 'bin', binName);
      if (process.platform !== 'win32') fs.chmodSync(platformBinary, 0o644);
      assert.match(run(process.execPath, [launcher, '--version'], root), /neex \d/);
      run(process.execPath, [launcher, 'ls'], root);
      fs.writeFileSync(path.join(root, 'build.cjs'), "const fs=require('node:fs'); fs.mkdirSync('dist',{recursive:true}); fs.writeFileSync('dist/out.txt',fs.readFileSync('src.txt'));\n");
      fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'consumer', scripts: { build: 'node build.cjs' } }));
      fs.writeFileSync(path.join(root, 'neex.json'), JSON.stringify({ tasks: { build: { outputs: ['dist/**'] } } }));
      fs.writeFileSync(path.join(root, 'src.txt'), 'source-v1');
      run(process.execPath, [launcher, 'build', '--summarize'], root);
      assert.equal(summary(root).executed, 1);
      fs.rmSync(path.join(root, 'dist'), { recursive: true });
      run(process.execPath, [launcher, 'build', '--summarize'], root);
      assert.equal(summary(root).cached, 1);
      assert.equal(fs.readFileSync(path.join(root, 'dist/out.txt'), 'utf8'), 'source-v1');
      fs.writeFileSync(path.join(root, 'dist/stale.txt'), 'stale');
      run(process.execPath, [launcher, 'build', '--summarize'], root);
      assert.equal(summary(root).cached, 1);
      assert.equal(fs.existsSync(path.join(root, 'dist/stale.txt')), false);
      fs.writeFileSync(path.join(root, 'src.txt'), 'source-v2');
      run(process.execPath, [launcher, 'build', '--summarize'], root);
      assert.equal(summary(root).executed, 1);
      for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) await cancellation(launcher, root, signal);
      console.log(`Launcher consumer passed (${platform}, ignoreScripts=${ignoreScripts})`);
    }
    const scaffolder = pack(path.join(repo, 'npm/create-neex'), temp);
    const consumer = path.join(temp, 'scaffolder');
    fs.mkdirSync(consumer);
    fs.writeFileSync(path.join(consumer, 'package.json'), '{"private":true}');
    run('npm', ['install', '--no-audit', '--no-fund', scaffolder], consumer);
    run(process.execPath, [path.join(consumer, 'node_modules/create-neex/dist/index.js'), '--version'], consumer);
    for (const template of ['next-express', 'next-hono']) {
      const config = path.join(consumer, 'node_modules/create-neex/templates', template, 'neex.json');
      assert.ok(JSON.parse(fs.readFileSync(config)).tasks.build);
    }
    console.log('Packed scaffolder contains runnable CLI and both templates');
  } finally { fs.rmSync(temp, { recursive: true, force: true }); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
