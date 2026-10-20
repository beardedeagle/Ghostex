/*
 * The App Store Connect API from CI with the workflow's API key: an ES256 token signed with node:crypto
 * and fetch, so the release helpers need no packages. Every non-2xx answer throws with Apple's error
 * text, which is what the helpers' "fail loudly" relies on.
 *
 * Env: GHOSTEX_ASC_KEY_PATH, GHOSTEX_ASC_KEY_ID, GHOSTEX_ASC_ISSUER_ID (the workflows' key step).
 */

import { sign } from 'node:crypto';
import { readFileSync } from 'node:fs';

const API = 'https://api.appstoreconnect.apple.com';
/** A token lives 10 minutes (the API allows 20); a new one is minted after 5 for long waits. */
const TOKEN_LIFETIME_SECONDS = 600;
const TOKEN_REFRESH_SECONDS = 300;

function base64url(value) {
  return Buffer.from(value).toString('base64url');
}

/** App Store Connect API token: ES256 JWT. */
export function ascToken({ keyPem, keyId, issuerId, now = Math.floor(Date.now() / 1000) }) {
  const header = base64url(JSON.stringify({ alg: 'ES256', kid: keyId, typ: 'JWT' }));
  const payload = base64url(
    JSON.stringify({
      iss: issuerId,
      iat: now,
      exp: now + TOKEN_LIFETIME_SECONDS,
      aud: 'appstoreconnect-v1',
    })
  );
  const signature = sign('sha256', Buffer.from(`${header}.${payload}`), {
    key: keyPem,
    dsaEncoding: 'ieee-p1363',
  });
  return `${header}.${payload}.${signature.toString('base64url')}`;
}

/** A client for the key named by the environment. */
export function ascClientFromEnv() {
  const { GHOSTEX_ASC_KEY_PATH: keyPath, GHOSTEX_ASC_KEY_ID: keyId, GHOSTEX_ASC_ISSUER_ID: issuerId } = process.env;
  if (!keyPath || !keyId || !issuerId)
    throw new Error('GHOSTEX_ASC_KEY_PATH, GHOSTEX_ASC_KEY_ID and GHOSTEX_ASC_ISSUER_ID are required');
  const keyPem = readFileSync(keyPath, 'utf8');
  let token = null;
  let mintedAt = 0;

  function currentToken() {
    const now = Math.floor(Date.now() / 1000);
    if (!token || now - mintedAt >= TOKEN_REFRESH_SECONDS) {
      token = ascToken({ keyPem, keyId, issuerId, now });
      mintedAt = now;
    }
    return token;
  }

  /** One call. `body` is sent as JSON; a 204 answers null. */
  async function request(method, url, body) {
    const response = await fetch(url.startsWith('https://') ? url : `${API}${url}`, {
      method,
      headers: {
        Authorization: `Bearer ${currentToken()}`,
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!response.ok) {
      throw new Error(`${method} ${url} answered ${response.status}: ${(await response.text()).slice(0, 1000)}`);
    }
    return response.status === 204 ? null : response.json();
  }

  /** Every page of a GET, following `links.next`. */
  async function list(url) {
    const data = [];
    const included = [];
    let next = url;
    while (next) {
      const page = await request('GET', next);
      data.push(...(page.data ?? []));
      included.push(...(page.included ?? []));
      next = page.links?.next ?? null;
    }
    return { data, included };
  }

  return { request, list };
}
