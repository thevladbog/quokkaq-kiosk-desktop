/**
 * Sync SemVer across package.json, src-tauri/Cargo.toml, and tauri.conf.json.
 * Usage: node scripts/bump-version.mjs [major|minor|patch]
 * Prints the new version to stdout.
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, '..');

const type = process.argv[2] || 'patch';

const pkgPath = path.join(root, 'package.json');
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
const versionParts = pkg.version.split('.').map(Number);

if (type === 'major') {
  versionParts[0] += 1;
  versionParts[1] = 0;
  versionParts[2] = 0;
} else if (type === 'minor') {
  versionParts[1] += 1;
  versionParts[2] = 0;
} else {
  versionParts[2] += 1;
}

const newVersion = versionParts.join('.');
pkg.version = newVersion;
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');

const cargoPath = path.join(root, 'src-tauri', 'Cargo.toml');
let cargo = fs.readFileSync(cargoPath, 'utf8');
if (!/^version = "[^"]+"/m.test(cargo)) {
  console.error('Could not find version = "..." in Cargo.toml');
  process.exit(1);
}
cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${newVersion}"`);
fs.writeFileSync(cargoPath, cargo);

const tauriConfPath = path.join(root, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
tauriConf.version = newVersion;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');

console.log(newVersion);
