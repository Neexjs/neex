const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { pathToFileURL } = require('node:url');

const cli = path.resolve(__dirname, '../dist/index.js');

function run(t, template, failInstall = false) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'neex-scaffold-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const bin = path.join(root, 'bin');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'pnpm'),
    '#!/bin/sh\nif [ "$1" = "--version" ]; then echo 10.11.0; exit 0; fi\nexit ' + (failInstall ? '7' : '0') + '\n',
    { mode: 0o755 });
  const result = spawnSync(process.execPath,
    [cli, 'test-app', '--template', template, '--no-git'], {
      cwd: root,
      env: { ...process.env, PATH: bin + path.delimiter + process.env.PATH },
      encoding: 'utf8', timeout: 15000,
    });
  assert.ifError(result.error);
  return { root, result };
}

test('invalid templates fail before creating a directory', { skip: process.platform === 'win32' }, t => {
  const { root, result } = run(t, '../../outside');
  assert.equal(result.status, 1);
  assert.match(result.stdout + result.stderr, /Unknown template/);
  assert.equal(fs.existsSync(path.join(root, 'test-app')), false);
  assert.doesNotMatch(result.stdout + result.stderr, /created successfully/);
});

test('install failures do not report success', { skip: process.platform === 'win32' }, t => {
  const { result } = run(t, 'next-express', true);
  assert.equal(result.status, 1);
  assert.match(result.stdout + result.stderr, /Dependency installation failed/);
  assert.doesNotMatch(result.stdout + result.stderr, /created successfully/);
});

for (const template of ['next-express', 'next-hono']) {
  test(`${template} produces valid workspace and cache configuration`, { skip: process.platform === 'win32' }, async t => {
    const { root, result } = run(t, template);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const project = path.join(root, 'test-app');
    const pkg = JSON.parse(fs.readFileSync(path.join(project, 'package.json')));
    const config = JSON.parse(fs.readFileSync(path.join(project, 'neex.json')));
    assert.equal(pkg.name, 'test-app');
    assert.equal(pkg.scripts.graph, 'neex graph');
    assert.equal(pkg.scripts.list, 'neex ls');
    assert.ok(fs.existsSync(path.join(project, 'pnpm-workspace.yaml')));
    assert.ok(fs.existsSync(path.join(project, '.gitignore')));
    assert.ok(fs.existsSync(path.join(project, 'eslint.config.mjs')));
    for (const relative of ['apps/api', 'apps/web', 'packages/ui', 'packages/utils']) {
      const child = JSON.parse(fs.readFileSync(path.join(project, relative, 'package.json')));
      assert.equal(child.scripts.lint, 'eslint .');
      assert.equal(child.scripts.typecheck, 'tsc --noEmit');
    }
    assert.deepEqual(config.tasks['@test-app/web#build'].outputs, ['.next/**', '!.next/cache/**', 'next-env.d.ts']);
    assert.deepEqual(config.tasks['@test-app/api#build'].outputs, ['dist/**']);
    assert.deepEqual(config.tasks['@test-app/utils#build'].outputs, ['dist/**']);
    assert.deepEqual(config.tasks.dev.dependsOn, ['^build']);
    if (template === 'next-hono') {
      const apiConfig = JSON.parse(fs.readFileSync(path.join(project, 'apps/api/tsconfig.json')));
      assert.deepEqual(apiConfig.compilerOptions.types, ['bun']);
    }
    // Express production uses plain Node, which cannot load .ts exports on
    // supported Node 18/20 runtimes. Verify shared utilities emit runnable JS.
    const utils = path.join(project, 'packages/utils');
    const compile = spawnSync(process.execPath, [require.resolve('typescript/bin/tsc'), '-p', utils],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(compile.status, 0, compile.stdout + compile.stderr);
    const utilsPkg = JSON.parse(fs.readFileSync(path.join(utils, 'package.json')));
    const module = await import(pathToFileURL(path.join(utils, utilsPkg.main)).href);
    assert.equal(module.formatDate(new Date('2026-10-02T00:00:00Z')), '2026-10-02T00:00:00.000Z');
    assert.ok(fs.existsSync(path.join(utils, utilsPkg.types)));
    assert.match(result.stdout, /pnpm dev/);
  });
}
