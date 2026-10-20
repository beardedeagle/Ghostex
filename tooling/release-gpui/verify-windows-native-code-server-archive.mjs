import { createHash } from 'node:crypto';
import { basename, resolve } from 'node:path';
import { readFile as readFileAsync } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { codeServerComponentNames } from './code-server-component-identity.mjs';
import {
  CODE_SERVER_ARCHIVE_CONTRACT,
  inspectCodeServerTarGz,
  parseCodeServerChecksumSidecar,
} from './verify-code-server-archive.mjs';

/*
 * CDXC:Release 2026-09-16 WHY:
 * The native Windows editor payload (windows-native-<arch>) is the win32 REH
 * build staged by build-windows-code-server.ps1, not the Linux archive the
 * shared code-server-archive-contract.json describes: its Node is lib/node.exe,
 * and it carries no POSIX executable bits. This sibling verifier shares the tar
 * walker and the sidecar parser with verify-code-server-archive.mjs but checks
 * the native layout, so the shared contract the Rust component store embeds
 * stays untouched. Since 2026-10-20 release builds install it on first use
 * instead of bundling it; the store re-checks the unpacked folder against these
 * entries.
 * SEE-ALSO: apps/desktop/scripts/build-windows-code-server.ps1,
 * .github/workflows/release-gpui-code-server-windows.yml,
 * verify_installed_windows_code_server in apps/desktop/src/component_store/code_server_archive.rs.
 */
export const WINDOWS_NATIVE_CODE_SERVER_FINGERPRINT_ENTRY = 'ghostex-build-fingerprint';

export const WINDOWS_NATIVE_CODE_SERVER_REQUIRED_ENTRIES = Object.freeze([
  'lib/node.exe',
  'lib/vscode/out/server-main.js',
  'lib/vscode/product.json',
  'out/node/entry.js',
  CODE_SERVER_ARCHIVE_CONTRACT.readinessEntry,
  'package.json',
  WINDOWS_NATIVE_CODE_SERVER_FINGERPRINT_ENTRY,
]);

const windowsNativePlatformPattern = /^windows-native-(x64|arm64)$/;

export function isWindowsNativeCodeServerPlatform(platform) {
  return windowsNativePlatformPattern.test(platform);
}

/** @param {{ archivePath: string, componentVersion: string, platform: string, sidecarPath?: string }} options */
export async function verifyWindowsNativeCodeServerArchive({ archivePath, componentVersion, platform, sidecarPath }) {
  if (!isWindowsNativeCodeServerPlatform(platform)) {
    throw new Error(`Unsupported native Windows code-server platform: ${platform}`);
  }
  const expectedArchiveName = codeServerComponentNames(componentVersion, platform).archiveName;
  if (basename(archivePath) !== expectedArchiveName) {
    throw new Error(
      `Native Windows code-server archive identity mismatch: expected ${expectedArchiveName}, got ${basename(archivePath)}`
    );
  }

  const resolvedSidecarPath = sidecarPath ?? `${archivePath}.sha256`;
  const [archiveBytes, sidecarContents] = await Promise.all([
    readFileAsync(archivePath),
    readFileAsync(resolvedSidecarPath, 'utf8'),
  ]);
  const expectedSha256 = parseCodeServerChecksumSidecar(sidecarContents, expectedArchiveName);
  const actualSha256 = createHash('sha256').update(archiveBytes).digest('hex');
  if (actualSha256 !== expectedSha256) {
    throw new Error(`Native Windows code-server archive checksum mismatch for ${expectedArchiveName}`);
  }

  const entries = inspectCodeServerTarGz(archiveBytes);
  for (const requiredEntry of WINDOWS_NATIVE_CODE_SERVER_REQUIRED_ENTRIES) {
    const entry = entries.get(requiredEntry);
    if (!entry || entry.size === 0) {
      throw new Error(`Native Windows code-server archive is missing required payload: ${requiredEntry}`);
    }
  }

  const nodeHeader = entries.get('lib/node.exe').contents.subarray(0, 2).toString('latin1');
  if (nodeHeader !== 'MZ') {
    throw new Error('Native Windows code-server archive lib/node.exe is not a Windows executable');
  }

  const fingerprint = entries.get(WINDOWS_NATIVE_CODE_SERVER_FINGERPRINT_ENTRY).contents.toString('utf8').trim();
  if (!/^[0-9a-f]{64}$/.test(fingerprint)) {
    throw new Error(
      `Native Windows code-server archive has a malformed ${WINDOWS_NATIVE_CODE_SERVER_FINGERPRINT_ENTRY}`
    );
  }

  try {
    JSON.parse(entries.get('lib/vscode/product.json').contents.toString('utf8'));
  } catch (error) {
    throw new Error(`Native Windows code-server archive lib/vscode/product.json is not valid JSON: ${error.message}`);
  }

  const readinessPayload = entries.get(CODE_SERVER_ARCHIVE_CONTRACT.readinessEntry).contents.toString('utf8');
  if (!readinessPayload.includes(CODE_SERVER_ARCHIVE_CONTRACT.readinessSignal)) {
    throw new Error(
      `Native Windows code-server archive lacks compiled ${CODE_SERVER_ARCHIVE_CONTRACT.readinessSignal} readiness signal`
    );
  }

  return { actualSha256, expectedArchiveName, fingerprint };
}

function readOption(args, name) {
  const index = args.indexOf(name);
  if (index === -1 || !args[index + 1]) throw new Error(`Missing required option ${name}`);
  return args[index + 1];
}

async function main() {
  const args = process.argv.slice(2);
  const result = await verifyWindowsNativeCodeServerArchive({
    archivePath: resolve(readOption(args, '--archive')),
    componentVersion: readOption(args, '--version'),
    platform: readOption(args, '--platform'),
  });
  process.stdout.write(
    `Verified ${result.expectedArchiveName} (${result.actualSha256}, fingerprint ${result.fingerprint})\n`
  );
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
