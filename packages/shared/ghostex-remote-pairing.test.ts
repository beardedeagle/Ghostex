import { describe, expect, test } from 'vitest';
import {
  PAIRING_LINK_PREFIX,
  encodeEasyConnectCode,
  encodeTailscaleCode,
  pairingLink,
  readPairingCode,
  type EasyConnectCode,
  type TailscaleCode,
} from './ghostex-remote-pairing';

const easyConnect: EasyConnectCode = {
  v: 1,
  address: 'tc1qexampleaddress',
  name: 'Studio',
  user: 'madda',
  port: 58744,
  sshPort: 22,
  secret: 's3cr3t',
  expiresAt: '2026-10-24T07:15:00.000Z',
};

const tailscale: TailscaleCode = { v: 1, name: 'Studio', host: 'studio.tail1234.ts.net', port: 22, user: 'madda' };

describe('pairingLink', () => {
  test('keeps the code after # so no web server sees it', () => {
    const code = encodeEasyConnectCode(easyConnect);
    const link = pairingLink(code);
    expect(link).toBe(`https://ghostex.dev/pair#${code}`);
    const url = new URL(link);
    expect(url.origin + url.pathname + url.search).toBe('https://ghostex.dev/pair');
    expect(url.hash).toBe(`#${code}`);
  });
});

describe('readPairingCode', () => {
  test('reads the bare codes older computers show', () => {
    expect(readPairingCode(encodeEasyConnectCode(easyConnect))).toEqual({ kind: 'easyConnect', code: easyConnect });
    expect(readPairingCode(encodeTailscaleCode(tailscale))).toEqual({ kind: 'tailscale', code: tailscale });
    expect(readPairingCode('tc1qexampleaddress')).toEqual({ kind: 'legacyAddress', address: 'tc1qexampleaddress' });
  });

  test('reads the links newer computers show', () => {
    const code = encodeEasyConnectCode(easyConnect);
    for (const link of [
      pairingLink(code),
      `  ${PAIRING_LINK_PREFIX}${code}\n`,
      `HTTPS://Ghostex.Dev/pair/#${code}`,
      `https://www.ghostex.dev/pair?from=camera#${code}`,
      `https://ghostex.dev/pair#${code.replace(':', '%3A')}`,
    ]) {
      expect(readPairingCode(link), link).toEqual({ kind: 'easyConnect', code: easyConnect });
    }
    expect(readPairingCode(pairingLink(encodeTailscaleCode(tailscale)))).toEqual({
      kind: 'tailscale',
      code: tailscale,
    });
  });

  test('refuses anything that is not a Ghostex pairing link', () => {
    const code = encodeEasyConnectCode(easyConnect);
    for (const link of [
      `https://ghostex.dev/pairs#${code}`,
      `https://ghostex.dev/pair/x#${code}`,
      `https://evil.example/pair#${code}`,
      `https://ghostex.dev.evil.example/pair#${code}`,
      'https://ghostex.dev/pair',
      'https://ghostex.dev/pair#tc1qexampleaddress',
      'https://ghostex.dev/pair#ghostex-ec1:not-base64!',
      'https://ghostex.dev/pair#%E0%A4%A',
    ]) {
      expect(readPairingCode(link), link).toBeNull();
    }
  });
});
