#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-19 WHY:
 * Uploads the Play build of the phone app (ghostex-android-play.aab from tooling/release-mobile/android.sh with GHOSTEX_ANDROID_DISTRIBUTION=play) to Google Play through the Play Developer API with a service account, as the TestFlight scripts do with the App Store Connect key: one edit that uploads the bundle and its R8 mapping, puts the release on one track, and commits. Play's API cannot create the app and refuses the very first bundle, so the first one is uploaded by hand in Play Console; until the app's first release is live, Play only takes `draft` releases. The workflow logs are public, so nothing here prints the service account, tokens or tester data.
 *
 * Usage:
 *   play-upload.mjs --version 10.28.0 --bundle <aab> --mapping <mapping.txt> --track internal --status draft [--rollout-fraction 0.2] [--validate-only]
 * Env: GOOGLE_PLAY_SERVICE_ACCOUNT_JSON (the service account's JSON key), GHOSTEX_PLAY_RELEASE_NOTES (optional en-US "What's new").
 */

import { createHash, createSign } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const PACKAGE_NAME = 'io.ghostex';
const API = `https://androidpublisher.googleapis.com/androidpublisher/v3/applications/${PACKAGE_NAME}`;
const UPLOAD_API = `https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications/${PACKAGE_NAME}`;
const TOKEN_URL = 'https://oauth2.googleapis.com/token';
const SCOPE = 'https://www.googleapis.com/auth/androidpublisher';
const TRACKS = new Set(['internal', 'alpha', 'beta', 'production']);
const STATUSES = new Set(['draft', 'completed', 'inProgress']);
const RELEASE_NOTES_MAX = 500;
const UPLOAD_TIMEOUT_MS = 20 * 60 * 1000;

const log = (message) => console.log(message);

/** Same numbering as release_gpui_build_number in tooling/release-gpui/common.sh. */
function versionCodeFor(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version ?? '');
  if (!match) throw new Error(`--version must be MAJOR.MINOR.PATCH, got ${version ?? '<none>'}`);
  return String(Number(match[1]) * 10000 + Number(match[2]) * 100 + Number(match[3]));
}

function readOptions() {
  const { values } = parseArgs({
    options: {
      version: { type: 'string' },
      bundle: { type: 'string' },
      mapping: { type: 'string' },
      track: { type: 'string' },
      status: { type: 'string' },
      'rollout-fraction': { type: 'string' },
      'validate-only': { type: 'boolean', default: false },
    },
  });
  const versionCode = versionCodeFor(values.version);
  if (!values.bundle) throw new Error('--bundle is required');
  if (!values.mapping) throw new Error('--mapping is required');
  if (!TRACKS.has(values.track))
    throw new Error(`--track must be one of ${[...TRACKS].join(', ')}, got ${values.track ?? '<none>'}`);
  if (!STATUSES.has(values.status))
    throw new Error(`--status must be one of ${[...STATUSES].join(', ')}, got ${values.status ?? '<none>'}`);
  let userFraction;
  if (values.status === 'inProgress') {
    userFraction = Number(values['rollout-fraction']);
    if (!(userFraction > 0 && userFraction < 1)) {
      throw new Error(
        `--rollout-fraction must be between 0 and 1 (exclusive) for a staged rollout, got ${values['rollout-fraction'] ?? '<none>'}`
      );
    }
  }
  const releaseNotes = (process.env.GHOSTEX_PLAY_RELEASE_NOTES ?? '').trim();
  if (releaseNotes.length > RELEASE_NOTES_MAX) {
    throw new Error(`release notes are ${releaseNotes.length} characters; Play allows ${RELEASE_NOTES_MAX}`);
  }
  return { ...values, versionCode, userFraction, releaseNotes, validateOnly: values['validate-only'] };
}

function readServiceAccount() {
  const raw = process.env.GOOGLE_PLAY_SERVICE_ACCOUNT_JSON ?? '';
  if (!raw.trim())
    throw new Error(
      "GOOGLE_PLAY_SERVICE_ACCOUNT_JSON is empty; add the service account's JSON key as that GitHub secret"
    );
  let account;
  try {
    account = JSON.parse(raw);
  } catch {
    throw new Error(
      'GOOGLE_PLAY_SERVICE_ACCOUNT_JSON is not valid JSON; paste the whole key file Google Cloud downloaded'
    );
  }
  if (account.type !== 'service_account' || !account.client_email || !account.private_key) {
    throw new Error(
      'GOOGLE_PLAY_SERVICE_ACCOUNT_JSON is not a service account key (needs type service_account, client_email and private_key)'
    );
  }
  return account;
}

async function accessToken(account) {
  const now = Math.floor(Date.now() / 1000);
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const unsigned = `${encode({ alg: 'RS256', typ: 'JWT' })}.${encode({
    iss: account.client_email,
    scope: SCOPE,
    aud: TOKEN_URL,
    iat: now,
    exp: now + 3600,
  })}`;
  const signature = createSign('RSA-SHA256').update(unsigned).sign(account.private_key).toString('base64url');
  const response = await fetch(TOKEN_URL, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer',
      assertion: `${unsigned}.${signature}`,
    }),
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok || !body.access_token) {
    throw new Error(
      `Google refused the service account (${response.status}): ${body.error_description ?? body.error ?? 'no reason given'}`
    );
  }
  return body.access_token;
}

function playClient(token) {
  return async function request(method, url, { body, contentType, timeoutMs = 120_000 } = {}) {
    const response = await fetch(url, {
      method,
      headers: { authorization: `Bearer ${token}`, ...(contentType ? { 'content-type': contentType } : {}) },
      body,
      signal: AbortSignal.timeout(timeoutMs),
    });
    const text = await response.text();
    let json = {};
    try {
      json = text ? JSON.parse(text) : {};
    } catch {
      json = { raw: text };
    }
    if (!response.ok) {
      const reason = json.error?.message ?? json.raw?.slice(0, 500) ?? 'no reason given';
      const hint = /draft app/i.test(reason)
        ? ' (the app has no live release yet: use status draft and roll the release out in Play Console)'
        : '';
      throw new Error(`${method} ${new URL(url).pathname} failed (${response.status}): ${reason}${hint}`);
    }
    return json;
  };
}

async function main() {
  const options = readOptions();
  const bundleBytes = readFileSync(options.bundle);
  const mappingBytes = readFileSync(options.mapping);
  const bundleSha256 = createHash('sha256').update(bundleBytes).digest('hex');
  log(
    `Bundle ${options.bundle}: ${bundleBytes.length} bytes, sha256 ${bundleSha256}, expected versionCode ${options.versionCode}`
  );

  const request = playClient(await accessToken(readServiceAccount()));
  const edit = await request('POST', `${API}/edits`, { body: '{}', contentType: 'application/json' });
  log(`Opened Play edit ${edit.id} for ${PACKAGE_NAME}`);

  // The installed app is above Play's 100 MB warning size (the native libraries are compressed in
  // the bundle and unpacked on install), so the upload has to acknowledge that warning.
  const bundle = await request(
    'POST',
    `${UPLOAD_API}/edits/${edit.id}/bundles?uploadType=media&ackBundleInstallationWarning=true`,
    { body: bundleBytes, contentType: 'application/octet-stream', timeoutMs: UPLOAD_TIMEOUT_MS }
  );
  if (String(bundle.versionCode) !== options.versionCode) {
    throw new Error(`Play read versionCode ${bundle.versionCode} from the bundle, expected ${options.versionCode}`);
  }
  if (bundle.sha256 !== bundleSha256) {
    throw new Error(`Play received a bundle with sha256 ${bundle.sha256}, expected ${bundleSha256}`);
  }
  log(`Uploaded bundle versionCode ${bundle.versionCode}`);

  await request(
    'POST',
    `${UPLOAD_API}/edits/${edit.id}/apks/${bundle.versionCode}/deobfuscationFiles/proguard?uploadType=media`,
    { body: mappingBytes, contentType: 'application/octet-stream', timeoutMs: UPLOAD_TIMEOUT_MS }
  );
  log('Uploaded the R8 mapping');

  const release = { name: options.version, versionCodes: [String(bundle.versionCode)], status: options.status };
  if (options.userFraction !== undefined) release.userFraction = options.userFraction;
  if (options.releaseNotes) release.releaseNotes = [{ language: 'en-US', text: options.releaseNotes }];
  await request('PUT', `${API}/edits/${edit.id}/tracks/${options.track}`, {
    body: JSON.stringify({ track: options.track, releases: [release] }),
    contentType: 'application/json',
  });
  const fraction = options.userFraction === undefined ? '' : ` to ${options.userFraction * 100}% of users`;
  log(`Set the ${options.track} track to release ${options.version} (${options.status}${fraction})`);

  if (options.validateOnly) {
    await request('POST', `${API}/edits/${edit.id}:validate`);
    await request('DELETE', `${API}/edits/${edit.id}`);
    log('Play validated the edit; nothing was committed (--validate-only)');
    return;
  }
  await request('POST', `${API}/edits/${edit.id}:commit`);
  log(`Committed: ${options.version} is on the ${options.track} track as ${options.status}`);
}

main().catch((error) => {
  console.error(`::error::${error.message}`);
  process.exit(1);
});
