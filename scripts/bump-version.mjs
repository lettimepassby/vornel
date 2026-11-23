#!/usr/bin/env node
// 一次性修改所有版本号，并刷新锁文件。
//
// 用法：node scripts/bump-version.mjs 0.2.0
//
// 会修改：
//   - Cargo.toml                             [workspace.package] version
//   - apps/vornel/src-tauri/tauri.conf.json  version
//   - apps/vornel/package.json               version
// 然后运行 cargo generate-lockfile 与 npm install --package-lock-only --ignore-scripts。
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const app = join(root, 'apps/vornel');

const version = (process.argv[2] ?? '').replace(/^v/, '');
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error('用法：node scripts/bump-version.mjs <版本号>，例如 0.2.0');
  process.exit(1);
}

// Cargo.toml：只改 [workspace.package] 段里的 version
const cargoPath = join(root, 'Cargo.toml');
const cargo = readFileSync(cargoPath, 'utf8');
const section = /(\[workspace\.package\][^[]*?^version\s*=\s*")[^"]*(")/m;
if (!section.test(cargo)) {
  console.error('Cargo.toml 里没有找到 [workspace.package] 的 version');
  process.exit(1);
}
writeFileSync(cargoPath, cargo.replace(section, `$1${version}$2`));

// JSON 文件：保留原有格式（两空格缩进、末尾换行）
function setJsonVersion(path) {
  const json = JSON.parse(readFileSync(path, 'utf8'));
  json.version = version;
  writeFileSync(path, JSON.stringify(json, null, 2) + '\n');
}
setJsonVersion(join(app, 'src-tauri/tauri.conf.json'));
setJsonVersion(join(app, 'package.json'));

const run = (cmd, args, cwd) => {
  console.log(`$ ${cmd} ${args.join(' ')}`);
  execFileSync(cmd, args, { cwd, stdio: 'inherit', shell: process.platform === 'win32' });
};
run('cargo', ['generate-lockfile'], root);
run('npm', ['install', '--package-lock-only', '--ignore-scripts'], app);

console.log(`\n版本号已改为 ${version}。接下来：
  git commit -am "版本 ${version}"
  推送到 main（或从 develop 合并到 main 后推送）
推送后 GitHub Actions 会自动并行打包 macOS（Apple 芯片 / Intel）与 Windows，生成更新清单并正式发布 v${version}。`);
