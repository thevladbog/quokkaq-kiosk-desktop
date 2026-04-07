#!/usr/bin/env node
/**
 * Cross-build the Go sidecar into src-tauri/binaries/ with the Tauri target triple name.
 */
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, '..');
const agentDir = path.join(root, 'agent');
const outDir = path.join(root, 'src-tauri', 'binaries');

const rustInfo = execSync('rustc -vV', { encoding: 'utf8' });
const hostMatch = rustInfo.match(/host: (\S+)/);
if (!hostMatch) {
  console.error('Could not parse rustc host triple');
  process.exit(1);
}
const triple = hostMatch[1];

const map = {
  'aarch64-apple-darwin': { goos: 'darwin', goarch: 'arm64' },
  'x86_64-apple-darwin': { goos: 'darwin', goarch: 'amd64' },
  'aarch64-unknown-linux-gnu': { goos: 'linux', goarch: 'arm64' },
  'x86_64-unknown-linux-gnu': { goos: 'linux', goarch: 'amd64' },
  'x86_64-pc-windows-msvc': { goos: 'windows', goarch: 'amd64' }
};

const target = map[triple];
if (!target) {
  console.error(`Unsupported host triple for agent build: ${triple}`);
  console.error('Add a mapping in scripts/build-agent.mjs or build the agent manually.');
  process.exit(1);
}

fs.mkdirSync(outDir, { recursive: true });

const ext = target.goos === 'windows' ? '.exe' : '';
const outName = `quokkaq-kiosk-agent-${triple}${ext}`;
const outPath = path.join(outDir, outName);

const env = {
  ...process.env,
  GOOS: target.goos,
  GOARCH: target.goarch,
  CGO_ENABLED: '0'
};

execSync(`go build -trimpath -ldflags="-s -w" -o ${JSON.stringify(outPath)} .`, {
  cwd: agentDir,
  env,
  stdio: 'inherit'
});

console.log(`Built agent: ${outPath}`);
