/**
 * CDXC:CodeEditor 2026-09-30 WHY:
 * A Linux dev build (`cargo xtask start`, `build-linux-app.sh` without GHOSTEX_ON_DEMAND_ASSETS) had no sealed manifest, and `.dependencies/code-server` is only a source checkout here, so the Code view had nothing to launch or install.
 * This seals a manifest naming the published code-server component whose identity matches the checkout, reading its size and digest from the release listing instead of downloading the 224 MB archive; the app then installs it on demand, or reuses the copy a packaged Ghostex already put in the shared component store.
 * SEE-ALSO: apps/desktop/scripts/build-linux-app.sh, tooling/release-gpui/code-server-component-identity.mjs, apps/desktop/src/app/helpers/source_server/code_server.rs.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { codeServerComponentIdentity, codeServerComponentNames } from './code-server-component-identity.mjs';
import { componentsGithubRepo } from './components-repo.mjs';
import { buildOnDemandManifestV2 } from './on-demand-manifest.mjs';
import { componentManifestRecord } from './publish-component.mjs';

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith('--') || !value || value.startsWith('--')) {
      throw new Error(
        'Usage: linux-dev-code-server-manifest.mjs --code-server-root DIR --platform linux-x64|linux-arm64 --version VERSION --cache-dir DIR --output PATH'
      );
    }
    options[key.slice(2)] = value;
  }
  for (const required of ['code-server-root', 'platform', 'version', 'cache-dir', 'output']) {
    if (!options[required]) throw new Error(`Missing --${required}`);
  }
  return options;
}

/** @returns {Promise<Map<string, any>>} */
async function releaseAssets(repository, tag) {
  const headers = { Accept: 'application/vnd.github+json' };
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  if (token) headers.Authorization = `Bearer ${token}`;
  const url = `https://api.github.com/repos/${repository}/releases/tags/${encodeURIComponent(tag)}`;
  const response = await fetch(url, { headers });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status} ${response.statusText}`);
  const release = /** @type {any} */ (await response.json());
  return new Map((release.assets ?? []).map((asset) => [asset.name, asset]));
}

function publishedAsset(assets, name, tag) {
  const asset = assets.get(name);
  const sha256 = /^sha256:([0-9a-f]{64})$/.exec(asset?.digest ?? '')?.[1];
  if (!asset || !sha256 || !Number.isSafeInteger(asset.size)) {
    throw new Error(`Component release ${tag} has no digest-bearing asset ${name}`);
  }
  return { assetName: name, sha256, sizeBytes: asset.size };
}

async function componentRecord({ codeServerRoot, platform, cacheDir }) {
  const { componentVersion } = await codeServerComponentIdentity({
    codeServerRoot,
  });
  const names = codeServerComponentNames(componentVersion, platform);
  const cachePath = path.join(cacheDir, `${names.downloadTag}-${platform}.json`);
  if (existsSync(cachePath)) return JSON.parse(readFileSync(cachePath, 'utf8'));

  const repository = componentsGithubRepo();
  const assets = await releaseAssets(repository, names.downloadTag);
  const archive = publishedAsset(assets, names.archiveName, names.downloadTag);
  const sidecar = publishedAsset(assets, `${names.archiveName}.sha256`, names.downloadTag);
  const record = componentManifestRecord({
    assets: [{ ...archive, platform }],
    checksumSidecars: [{ ...sidecar, platform }],
    component: 'code-server',
    componentVersion,
    downloadTag: names.downloadTag,
    githubRepo: repository,
  });
  mkdirSync(cacheDir, { recursive: true });
  writeFileSync(cachePath, `${JSON.stringify(record, null, 2)}\n`);
  return record;
}

const options = parseArgs(process.argv.slice(2));
const record = await componentRecord({
  codeServerRoot: options['code-server-root'],
  platform: options.platform,
  cacheDir: options['cache-dir'],
});
const manifest = buildOnDemandManifestV2({
  version: options.version,
  assets: {},
  components: { 'code-server': record },
});
mkdirSync(path.dirname(options.output), { recursive: true });
writeFileSync(options.output, `${JSON.stringify(manifest, null, 2)}\n`);
process.stdout.write(`Sealed Linux dev manifest for code-server ${record.componentVersion}: ${options.output}\n`);
