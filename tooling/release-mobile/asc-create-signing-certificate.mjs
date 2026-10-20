#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-11 WHY:
 * The reused iOS signing certificate's private key is made and kept on the computer that runs tooling/release-mobile/ios-signing-certificate.sh; only its CSR (a public key) comes here. This step exists because the App Store Connect API key lives only in GitHub secrets: it asks Apple for the Apple Development certificate and hands back the certificate, which is public, as an artifact. Nothing secret is written to the public log or artifact.
 *
 * Usage: asc-create-signing-certificate.mjs <teamId> <out dir>
 * Env: GHOSTEX_IOS_CSR_BASE64 (the CSR PEM, base64 on one line), plus the key env in asc-api.mjs.
 */

import { X509Certificate } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { ascClientFromEnv } from './asc-api.mjs';
import { classifyCertificate } from './asc-revoke-ci-certificates.mjs';

const CSR_PEM = /^-----BEGIN CERTIFICATE REQUEST-----\r?\n[A-Za-z0-9+/=\r\n]+-----END CERTIFICATE REQUEST-----\r?\n?$/;

async function main(argv) {
  const [teamId, outDir] = argv;
  if (!/^[A-Z0-9]{10}$/.test(teamId ?? '') || !outDir)
    throw new Error('usage: asc-create-signing-certificate.mjs <teamId> <out dir>');
  const csr = Buffer.from(process.env.GHOSTEX_IOS_CSR_BASE64 ?? '', 'base64').toString('utf8');
  if (!CSR_PEM.test(csr)) throw new Error('GHOSTEX_IOS_CSR_BASE64 is not a base64-encoded PEM certificate request');

  const client = ascClientFromEnv();
  const created = await client.request('POST', '/v1/certificates', {
    data: {
      type: 'certificates',
      attributes: { certificateType: 'DEVELOPMENT', csrContent: csr },
    },
  });
  const resource = created?.data;
  const verdict = classifyCertificate(resource, teamId);
  const attributes = resource?.attributes ?? {};
  console.log(
    `Created certificate ${resource?.id} "${attributes.name ?? ''}" serial ${attributes.serialNumber ?? '?'} expires ${attributes.expirationDate ?? '?'} ${verdict.subject ?? ''}`
  );
  if (!verdict.ci)
    throw new Error(
      `Apple issued a certificate the CI rules do not recognise (${verdict.reason}); check it in App Store Connect`
    );

  const x509 = new X509Certificate(Buffer.from(attributes.certificateContent, 'base64'));
  mkdirSync(outDir, { recursive: true });
  writeFileSync(path.join(outDir, 'certificate.pem'), x509.toString());
  writeFileSync(
    path.join(outDir, 'certificate.json'),
    `${JSON.stringify(
      {
        id: resource.id,
        name: attributes.name,
        serialNumber: attributes.serialNumber,
        expirationDate: attributes.expirationDate,
        sha256: x509.fingerprint256,
      },
      null,
      2
    )}\n`
  );
  console.log(`SHA-256 ${x509.fingerprint256}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
