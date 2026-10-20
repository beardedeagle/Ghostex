#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-11 DECISION:
 * User: iOS builds reuse one long-lived signing certificate, like the user's Mac, so runs stop creating and revoking certificates (Apple emails about every revoke). This supersedes revoking each run's own certificate afterwards. The before-run cleanup stays only because it can never hit the reused certificate. Only revoke certificates whose name shows the API made them for CI; never touch certificates named after a person, distribution certificates, or anything that cannot be positively identified as CI-made. Log what is revoked.
 *
 * CDXC:Release 2026-10-11 WHY:
 * `xcodebuild -allowProvisioningUpdates` with the App Store Connect API key finds no development identity on a fresh runner, so every run creates an "Apple Development: Created via API (<API key ID>)" certificate whose private key dies with the runner. The 10.19.0 TestFlight run (38025945451) failed on "Your account has reached the maximum number of certificates". A certificate is CI-made only when all of these hold: its type is DEVELOPMENT or IOS_DEVELOPMENT, and the X.509 subject inside its content (not only the portal's display name) has a CN of exactly "Apple Development: Created via API (" + 10 uppercase letters or digits + ")" and OU = the team. The parenthesised value is the key ID, not the team: run 38099870415 kept every certificate when the CN was matched against the team ID, and older keys' certificates carry their own IDs.
 *
 * CDXC:Release 2026-10-11 WHY:
 * The reused certificate has the same CI name, so `--keep-cert` names it by its own PEM (from the p12 secret) and it is matched by SHA-256 fingerprint. Only CI certificates that expire before it are revoked: leftovers from before the reuse are older, while a certificate issued after it (a renewal whose p12 is not stored yet, or one a run made anyway) is newer and is kept. If the reused certificate is not in the listing (revoked or expired), nothing is revoked and the step fails.
 *
 * Usage: asc-revoke-ci-certificates.mjs <teamId> --keep-cert <reused certificate PEM> [--dry-run]
 * Env: GHOSTEX_ASC_KEY_PATH, GHOSTEX_ASC_KEY_ID, GHOSTEX_ASC_ISSUER_ID (the workflow's key step).
 */

import { X509Certificate } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { ascClientFromEnv } from './asc-api.mjs';

const DEVELOPMENT_TYPES = new Set(['DEVELOPMENT', 'IOS_DEVELOPMENT']);
/** The parenthesised value is the ID of the API key that made the certificate. */
const CI_COMMON_NAME = /^Apple Development: Created via API \([A-Z0-9]{10}\)$/;

/** The subject's attributes, e.g. { CN: '…', OU: '…' }. A repeated attribute makes the subject unreadable. */
function subjectFields(x509) {
  const fields = {};
  for (const line of x509.subject.split('\n')) {
    const at = line.indexOf('=');
    if (at <= 0) return null;
    const key = line.slice(0, at);
    if (key in fields) return null;
    fields[key] = line.slice(at + 1);
  }
  return fields;
}

/**
 * Whether one `certificates` resource is a CI-made development certificate, and why not when it is
 * not. Anything this cannot read is kept. `x509` is the parsed certificate whenever it was readable.
 */
export function classifyCertificate(resource, teamId) {
  const attributes = resource?.attributes ?? {};
  if (!DEVELOPMENT_TYPES.has(attributes.certificateType)) {
    return {
      ci: false,
      reason: `type ${attributes.certificateType ?? 'unknown'}`,
    };
  }
  if (!attributes.certificateContent) return { ci: false, reason: 'no certificate content' };
  let x509;
  try {
    x509 = new X509Certificate(Buffer.from(attributes.certificateContent, 'base64'));
  } catch (error) {
    return { ci: false, reason: `unreadable certificate (${error.message})` };
  }
  const fields = subjectFields(x509);
  if (!fields) return { ci: false, x509, reason: 'unreadable subject' };
  const subject = `CN "${fields.CN ?? ''}" OU "${fields.OU ?? ''}"`;
  if (!CI_COMMON_NAME.test(fields.CN ?? '')) {
    return {
      ci: false,
      x509,
      subject,
      reason: 'CN is not a Created via API one',
    };
  }
  if (fields.OU !== teamId) {
    return { ci: false, x509, subject, reason: `OU is not team ${teamId}` };
  }
  return { ci: true, x509, subject };
}

/** The reused signing certificate, from the PEM the workflow extracted from the p12 secret. */
function readKeptCertificate(pemPath) {
  const x509 = new X509Certificate(readFileSync(pemPath));
  return { fingerprint: x509.fingerprint256, expires: new Date(x509.validTo) };
}

async function listDevelopmentCertificates(client) {
  const fields = 'name,certificateType,serialNumber,expirationDate,certificateContent';
  const { data } = await client.list(
    `/v1/certificates?filter[certificateType]=${[...DEVELOPMENT_TYPES].join(',')}&fields[certificates]=${fields}&limit=200`
  );
  return data;
}

async function main(argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      'keep-cert': { type: 'string' },
      'dry-run': { type: 'boolean' },
    },
  });
  const teamId = positionals[0];
  const dryRun = values['dry-run'] === true;
  if (!/^[A-Z0-9]{10}$/.test(teamId ?? '') || !values['keep-cert'])
    throw new Error(`usage: asc-revoke-ci-certificates.mjs <teamId> --keep-cert <reused certificate PEM> [--dry-run]`);
  const kept = readKeptCertificate(values['keep-cert']);
  const client = ascClientFromEnv();
  const certificates = await listDevelopmentCertificates(client);
  const verdicts = certificates.map((resource) => ({
    resource,
    verdict: classifyCertificate(resource, teamId),
  }));
  if (!verdicts.some(({ verdict }) => verdict.x509?.fingerprint256 === kept.fingerprint))
    throw new Error(
      `the reused signing certificate (SHA-256 ${kept.fingerprint}) is not among the ${certificates.length} development certificate(s); nothing was revoked. Renew it with tooling/release-mobile/ios-signing-certificate.sh.`
    );
  console.log(`Reused signing certificate: SHA-256 ${kept.fingerprint}, expires ${kept.expires.toISOString()}`);
  let revoked = 0;
  for (const { resource, verdict } of verdicts) {
    const { attributes = {} } = resource;
    const label = `${resource.id} "${attributes.name ?? ''}" serial ${attributes.serialNumber ?? '?'} expires ${attributes.expirationDate ?? '?'}`;
    if (!verdict.ci) {
      console.log(`kept ${label}${verdict.subject ? ` ${verdict.subject}` : ''}: ${verdict.reason}`);
      continue;
    }
    if (verdict.x509.fingerprint256 === kept.fingerprint) {
      console.log(`kept ${label} ${verdict.subject}: the reused signing certificate`);
      continue;
    }
    if (!(new Date(verdict.x509.validTo) < kept.expires)) {
      console.log(`kept ${label} ${verdict.subject}: not older than the reused signing certificate`);
      continue;
    }
    if (dryRun) {
      console.log(`would revoke ${label} ${verdict.subject}`);
      continue;
    }
    await client.request('DELETE', `/v1/certificates/${encodeURIComponent(resource.id)}`);
    revoked += 1;
    console.log(`revoked ${label} ${verdict.subject}`);
  }
  console.log(
    `${dryRun ? 'Dry run: ' : ''}${revoked} CI development certificate(s) revoked, ${certificates.length} development certificate(s) seen.`
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
