/**
 * Ghostex over-the-air update server: the Expo Updates protocol v1 for the phone app, on a
 * Cloudflare Worker with one R2 bucket.
 *
 * The phone (expo-updates) asks GET /v1/manifest/<channel> with its platform and runtime version
 * in headers and gets the newest update for exactly that runtime version, or 204 when it already
 * runs it or there is none. Assets live under their SHA-256, so a URL never changes meaning.
 *
 * The Worker holds no signing key. CI signs every manifest and directive with the private key in
 * the release secrets and uploads the signed record through the /v1/admin routes, which need the
 * upload token. A stolen upload token can therefore not make phones run anything: they verify the
 * signature against the certificate inside the app before downloading a single asset.
 *
 * R2 layout:
 *   assets/<sha256 hex>                                       the bytes, with their content type
 *   updates/<channel>/<platform>/<runtime>/current.json       the record phones are served
 *   updates/<channel>/<platform>/<runtime>/history/<name>.json every record ever published
 * A record is { kind: 'update' | 'rollBackToEmbedded', version, createdAt, body, signature }:
 * `body` is the exact JSON text that was signed and `signature` the expo-signature header value.
 *
 * SEE-ALSO: tooling/release-mobile/ota-publish.mjs (the only writer), apps/mobile/app/app.config.js
 * (CDXC:Release 2026-10-23 DECISION), apps/mobile/app/src/updates/overTheAir.ts (/v1/status).
 */

const PLATFORMS = new Set(['android', 'ios']);
const NAME = /^[A-Za-z0-9._-]{1,128}$/;
const SHA256_HEX = /^[0-9a-f]{64}$/;
const PROTOCOL_HEADERS = {
  'expo-protocol-version': '1',
  'expo-sfv-version': '0',
  'cache-control': 'private, max-age=0',
};

export default {
  async fetch(request, env) {
    try {
      return await route(request, env);
    } catch (error) {
      return json({ error: error instanceof Error ? error.message : String(error) }, 500);
    }
  },
};

async function route(request, env) {
  const url = new URL(request.url);
  const parts = url.pathname.split('/').filter(Boolean);
  const method = request.method;
  if (parts[0] !== 'v1') return json({ error: 'Not found.' }, 404);

  if (parts[1] === 'manifest' && parts.length === 3 && method === 'GET') return manifest(request, env, parts[2]);
  if (parts[1] === 'assets' && parts.length === 3 && (method === 'GET' || method === 'HEAD')) {
    return asset(env, parts[2], method);
  }
  if (parts[1] === 'status' && parts.length === 5 && method === 'GET') return status(env, parts[2], parts[3], parts[4]);

  if (parts[1] === 'admin') {
    if (!(await authorized(request, env))) return json({ error: 'Unauthorized.' }, 401);
    if (parts[2] === 'assets' && parts.length === 4 && method === 'PUT') return putAsset(request, env, parts[3]);
    if (parts[2] === 'updates' && parts.length === 6 && method === 'POST') {
      return putRecord(request, env, parts[3], parts[4], parts[5]);
    }
    if (parts[2] === 'updates' && parts.length === 7 && parts[6] === 'history' && method === 'GET') {
      return listHistory(env, parts[3], parts[4], parts[5]);
    }
    if (parts[2] === 'updates' && parts.length === 8 && parts[6] === 'history' && method === 'GET') {
      return getHistory(env, parts[3], parts[4], parts[5], parts[7]);
    }
  }
  return json({ error: 'Not found.' }, 404);
}

function json(value, status = 200, headers = {}) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store', ...headers },
  });
}

function updatePrefix(channel, platform, runtime) {
  if (!NAME.test(channel) || !PLATFORMS.has(platform) || !NAME.test(runtime)) return null;
  return `updates/${channel}/${platform}/${runtime}`;
}

async function readRecord(env, key) {
  const object = await env.OTA.get(key);
  return object === null ? null : JSON.parse(await object.text());
}

function noUpdate() {
  return new Response(null, { status: 204, headers: PROTOCOL_HEADERS });
}

/** The Expo Updates protocol v1 manifest request. */
async function manifest(request, env, channel) {
  if (request.headers.get('expo-protocol-version') !== '1') {
    return json({ error: 'Only expo-updates protocol version 1 is served.' }, 406);
  }
  const platform = request.headers.get('expo-platform') ?? '';
  const runtime = request.headers.get('expo-runtime-version') ?? '';
  const prefix = updatePrefix(channel, platform, runtime);
  if (prefix === null) return json({ error: 'Unknown channel, platform or runtime version.' }, 400);

  const record = await readRecord(env, `${prefix}/current.json`);
  if (record === null) return noUpdate();

  const current = (request.headers.get('expo-current-update-id') ?? '').toLowerCase();
  let partName;
  if (record.kind === 'update') {
    if (current !== '' && current === JSON.parse(record.body).id.toLowerCase()) return noUpdate();
    partName = 'manifest';
  } else if (record.kind === 'rollBackToEmbedded') {
    const embedded = (request.headers.get('expo-embedded-update-id') ?? '').toLowerCase();
    if (current !== '' && current === embedded) return noUpdate();
    partName = 'directive';
  } else {
    throw new Error(`Unknown record kind at ${prefix}: ${record.kind}`);
  }

  const boundary = `ghostex-${crypto.randomUUID()}`;
  const body =
    `--${boundary}\r\n` +
    `content-disposition: form-data; name="${partName}"\r\n` +
    'content-type: application/json; charset=utf-8\r\n' +
    `expo-signature: ${record.signature}\r\n` +
    '\r\n' +
    `${record.body}\r\n` +
    `--${boundary}--\r\n`;
  return new Response(body, {
    status: 200,
    headers: { ...PROTOCOL_HEADERS, 'content-type': `multipart/mixed; boundary=${boundary}` },
  });
}

async function asset(env, hash, method) {
  if (!SHA256_HEX.test(hash)) return json({ error: 'Not found.' }, 404);
  const object = method === 'HEAD' ? await env.OTA.head(`assets/${hash}`) : await env.OTA.get(`assets/${hash}`);
  if (object === null) return json({ error: 'Not found.' }, 404);
  const headers = {
    'content-type': object.httpMetadata?.contentType ?? 'application/octet-stream',
    'cache-control': 'public, max-age=31536000, immutable',
    etag: object.httpEtag,
  };
  return new Response(method === 'HEAD' ? null : object.body, { status: 200, headers });
}

/** Which release a runtime version gets over the air; the APK updater asks before offering an APK. */
async function status(env, channel, platform, runtime) {
  const prefix = updatePrefix(channel, platform, runtime);
  if (prefix === null) return json({ error: 'Unknown channel, platform or runtime version.' }, 400);
  const record = await readRecord(env, `${prefix}/current.json`);
  if (record === null || record.kind !== 'update') {
    return json({ error: 'No update for this runtime version.' }, 404, { 'cache-control': 'public, max-age=60' });
  }
  return json({ version: record.version, createdAt: record.createdAt }, 200, { 'cache-control': 'public, max-age=60' });
}

async function authorized(request, env) {
  const expected = env.UPLOAD_TOKEN;
  if (typeof expected !== 'string' || expected.length < 32) return false;
  const given = (request.headers.get('authorization') ?? '').replace(/^Bearer\s+/i, '');
  const encoder = new TextEncoder();
  const [a, b] = await Promise.all([
    crypto.subtle.digest('SHA-256', encoder.encode(given)),
    crypto.subtle.digest('SHA-256', encoder.encode(expected)),
  ]);
  return crypto.subtle.timingSafeEqual(a, b);
}

function hex(buffer) {
  return [...new Uint8Array(buffer)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

async function putAsset(request, env, hash) {
  if (!SHA256_HEX.test(hash)) return json({ error: 'The asset name must be its SHA-256 in hex.' }, 400);
  const bytes = await request.arrayBuffer();
  const actual = hex(await crypto.subtle.digest('SHA-256', bytes));
  if (actual !== hash) return json({ error: `The body hashes to ${actual}, not ${hash}.` }, 400);
  await env.OTA.put(`assets/${hash}`, bytes, {
    httpMetadata: { contentType: request.headers.get('content-type') ?? 'application/octet-stream' },
  });
  return json({ stored: hash }, 201);
}

/** Publishes a signed record: it becomes what phones on that runtime version are served. */
async function putRecord(request, env, channel, platform, runtime) {
  const prefix = updatePrefix(channel, platform, runtime);
  if (prefix === null) return json({ error: 'Unknown channel, platform or runtime version.' }, 400);
  const record = await request.json();
  if (record.kind !== 'update' && record.kind !== 'rollBackToEmbedded') {
    return json({ error: `Unknown record kind ${record.kind}.` }, 400);
  }
  if (typeof record.body !== 'string' || typeof record.signature !== 'string' || typeof record.createdAt !== 'string') {
    return json({ error: 'A record needs body, signature and createdAt strings.' }, 400);
  }
  const body = JSON.parse(record.body);
  if (record.kind === 'update') {
    if (body.runtimeVersion !== runtime) {
      return json({ error: `The manifest is for runtime ${body.runtimeVersion}, not ${runtime}.` }, 400);
    }
    // Never point phones at an update whose files are not all here yet.
    for (const entry of [body.launchAsset, ...body.assets]) {
      const name = new URL(entry.url).pathname.split('/').pop();
      if ((await env.OTA.head(`assets/${name}`)) === null) {
        return json({ error: `Asset ${name} has not been uploaded.` }, 409);
      }
    }
  }
  const stored = JSON.stringify({
    kind: record.kind,
    version: typeof record.version === 'string' ? record.version : null,
    createdAt: record.createdAt,
    body: record.body,
    signature: record.signature,
  });
  const name = `${record.createdAt.replace(/[^0-9A-Za-z]/g, '')}-${record.kind}`;
  await env.OTA.put(`${prefix}/history/${name}.json`, stored, { httpMetadata: { contentType: 'application/json' } });
  await env.OTA.put(`${prefix}/current.json`, stored, { httpMetadata: { contentType: 'application/json' } });
  return json({ published: `${prefix}/current.json`, history: name }, 201);
}

async function listHistory(env, channel, platform, runtime) {
  const prefix = updatePrefix(channel, platform, runtime);
  if (prefix === null) return json({ error: 'Unknown channel, platform or runtime version.' }, 400);
  const names = [];
  let cursor;
  do {
    const page = await env.OTA.list({ prefix: `${prefix}/history/`, cursor });
    for (const object of page.objects) names.push(object.key.slice(`${prefix}/history/`.length).replace(/\.json$/, ''));
    cursor = page.truncated ? page.cursor : undefined;
  } while (cursor !== undefined);
  return json({ history: names.sort() });
}

async function getHistory(env, channel, platform, runtime, name) {
  const prefix = updatePrefix(channel, platform, runtime);
  if (prefix === null || !NAME.test(name)) return json({ error: 'Not found.' }, 404);
  const record = await readRecord(env, `${prefix}/history/${name}.json`);
  return record === null ? json({ error: 'Not found.' }, 404) : json(record);
}
