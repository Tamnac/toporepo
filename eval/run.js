#!/usr/bin/env node
// Cross-platform eval runner for toporepo.
//
// Usage:
//   node eval/run.js                         # run all tests from default file
//   node eval/run.js test_id [test_id...]     # run specific tests
//   EVAL_TESTS=path.json node eval/run.js     # use a different test file

const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const root = path.resolve(__dirname, '..');
process.chdir(root);

const bin = path.join(
  root,
  'target',
  'release',
  process.platform === 'win32' ? 'toporepo.exe' : 'toporepo',
);

function newestMtime(dir) {
  let newest = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      newest = Math.max(newest, newestMtime(full));
    } else {
      newest = Math.max(newest, fs.statSync(full).mtimeMs);
    }
  }
  return newest;
}

function ensureBuilt() {
  const binExists = fs.existsSync(bin);
  const binMtime = binExists ? fs.statSync(bin).mtimeMs : 0;
  const srcMtime = newestMtime(path.join(root, 'src'));
  if (!binExists || srcMtime > binMtime) {
    console.log('Building toporepo...');
    const build = spawnSync('cargo', ['build', '--release', '--quiet'], {
      cwd: root,
      stdio: 'inherit',
      shell: process.platform === 'win32',
    });
    if (build.status !== 0) {
      process.exit(build.status ?? 1);
    }
  }
}

function repoExists(repo) {
  try {
    return fs.statSync(path.resolve(root, repo)).isDirectory();
  } catch {
    return false;
  }
}

function runMap(test) {
  const run = spawnSync(
    bin,
    ['map', test.repo, '-q', test.query, '-n', String(test.tokens)],
    {
      cwd: root,
      encoding: 'utf8',
      shell: false,
      windowsHide: true,
    },
  );
  return `${run.stdout ?? ''}${run.stderr ?? ''}`;
}

ensureBuilt();

const testsPath = path.resolve(root, process.env.EVAL_TESTS || path.join('eval', 'tests.json'));
if (!fs.existsSync(testsPath)) {
  console.error(`Test file not found: ${testsPath}`);
  console.error('Set EVAL_TESTS= or create eval/tests.json.');
  process.exit(1);
}

const tests = JSON.parse(fs.readFileSync(testsPath, 'utf8'));
const filters = new Set(process.argv.slice(2));
let passed = 0;
let failed = 0;
let skipped = 0;
const failIds = [];

for (const test of tests) {
  if (filters.size > 0 && !filters.has(test.id)) {
    continue;
  }

  if (!repoExists(test.repo)) {
    console.log(`  SKIP  ${test.id}  (repo not found: ${test.repo})`);
    skipped += 1;
    continue;
  }

  const output = runMap(test);
  const details = [];
  for (const expected of test.expect_files ?? []) {
    if (!output.includes(expected)) {
      details.push(`  missing file: ${expected}`);
    }
  }
  for (const expected of test.expect_strings ?? []) {
    if (!output.includes(expected)) {
      details.push(`  missing string: ${expected}`);
    }
  }

  if (details.length === 0) {
    console.log(`  PASS  ${test.id}`);
    passed += 1;
  } else {
    console.log(`  FAIL  ${test.id}  (${test.note})`);
    for (const detail of details) {
      console.log(detail);
    }
    console.log('');
    failed += 1;
    failIds.push(test.id);
  }
}

const total = passed + failed;
console.log('');
if (skipped > 0) {
  console.log(`(${skipped} skipped - missing repos)`);
}
console.log(`${passed}/${total} passed`);

if (failed > 0) {
  console.log(`Failed: ${failIds.join(' ')}`);
  process.exit(1);
}
