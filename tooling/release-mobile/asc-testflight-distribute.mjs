#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-11 DECISION:
 * User: TestFlight builds go straight to the external Discord testers, done through the App Store Connect API from CI with the release key instead of clicking through App Store Connect. For one version: wait until the build has processed, set its "What to Test", add it to the external group, and submit it for Beta App Review unless it is already submitted or approved; log every step and fail loudly on API errors.
 *
 * CDXC:Release 2026-10-11 WHY:
 * The workflow logs are public (maddada/Ghostex is a public repository), so `list` prints group names, states and whether review details are filled in, never a public TestFlight link, tester data or review contact details. Export compliance is answered by `ITSAppUsesNonExemptEncryption: false` in the app's Info.plist (app.json), which App Store Connect records as the build's `usesNonExemptEncryption`; a build without that answer stops here rather than having an answer sent on the user's behalf.
 *
 * Usage:
 *   asc-testflight-distribute.mjs list [--version 10.19.0] [--build 101900]
 *   asc-testflight-distribute.mjs distribute --version 10.19.0 --group "<exact group name>" [--build 101900] [--timeout-minutes 90] [--dry-run]
 * "What to Test" comes from GHOSTEX_WHAT_TO_TEST. Key env: see asc-api.mjs.
 */

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { ascClientFromEnv } from './asc-api.mjs';

const BUNDLE_ID = 'com.maddada.ghostex.ios';
const LOCALE = 'en-US';
const POLL_SECONDS = 30;
const WHAT_TO_TEST_MAX = 4000;
const RECENT_BUILDS = 10;
/** Submission states that need nothing more from us. */
const REVIEW_PENDING_OR_DONE = new Set(['WAITING_FOR_REVIEW', 'IN_REVIEW', 'APPROVED']);
/** External build states past the point of submitting. */
const EXTERNAL_SUBMITTED_OR_DONE = new Set([
  'WAITING_FOR_BETA_REVIEW',
  'IN_BETA_REVIEW',
  'BETA_APPROVED',
  'IN_BETA_TESTING',
]);

const log = (message) => console.log(message);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Same numbering as release_gpui_build_number in tooling/release-gpui/common.sh. */
export function buildNumberFor(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version ?? '');
  if (!match) throw new Error(`version must be MAJOR.MINOR.PATCH, got ${version ?? '<none>'}`);
  return String(Number(match[1]) * 10000 + Number(match[2]) * 100 + Number(match[3]));
}

async function findApp(client) {
  const { data } = await client.request(
    'GET',
    `/v1/apps?filter[bundleId]=${encodeURIComponent(BUNDLE_ID)}&fields[apps]=name,bundleId&limit=10`
  );
  const app = data.find((candidate) => candidate.attributes?.bundleId === BUNDLE_ID);
  if (!app) throw new Error(`no App Store Connect app with bundle ID ${BUNDLE_ID}`);
  log(`App: ${app.attributes.name} (${app.id}, ${BUNDLE_ID})`);
  return app;
}

async function listGroups(client, appId) {
  const { data } = await client.list(
    `/v1/betaGroups?filter[app]=${appId}&fields[betaGroups]=name,isInternalGroup,publicLinkEnabled,createdDate&limit=200`
  );
  return data;
}

async function groupBuildIds(client, groupId) {
  const { data } = await client.list(`/v1/betaGroups/${groupId}/relationships/builds?limit=200`);
  return new Set(data.map((linkage) => linkage.id));
}

function preReleaseVersionOf(build, included) {
  const id = build.relationships?.preReleaseVersion?.data?.id;
  return included.find((resource) => resource.type === 'preReleaseVersions' && resource.id === id)?.attributes?.version;
}

/** The build with this build number in this version, or null while App Store Connect does not list it yet. */
async function findBuild(client, appId, version, buildNumber) {
  const { data, included } = await client.list(
    `/v1/builds?filter[app]=${appId}&filter[version]=${encodeURIComponent(buildNumber)}&include=preReleaseVersion&limit=50`
  );
  const matches = data.filter((build) => preReleaseVersionOf(build, included) === version);
  if (matches.length > 1) throw new Error(`${matches.length} builds match ${version} (${buildNumber})`);
  return matches[0] ?? null;
}

async function betaDetail(client, buildId) {
  const { data } = await client.request('GET', `/v1/builds/${buildId}/buildBetaDetail`);
  return data?.attributes ?? {};
}

async function reviewSubmission(client, buildId) {
  const { data } = await client.list(`/v1/betaAppReviewSubmissions?filter[build]=${buildId}&limit=10`);
  return data[0] ?? null;
}

async function buildLocalization(client, buildId) {
  const { data } = await client.list(`/v1/builds/${buildId}/betaBuildLocalizations?limit=50`);
  return data.find((resource) => resource.attributes?.locale === LOCALE) ?? null;
}

function describeEncryption(value) {
  if (value === false) return 'answered: no non-exempt encryption';
  if (value === true) return 'answered: uses non-exempt encryption';
  return 'not answered';
}

async function describeBuild(client, build, version, groupsById) {
  const a = build.attributes ?? {};
  const detail = await betaDetail(client, build.id);
  const submission = await reviewSubmission(client, build.id);
  const localization = await buildLocalization(client, build.id);
  const groups = [...groupsById.entries()]
    .filter(([, group]) => group.buildIds.has(build.id))
    .map(([, group]) => group.name);
  log(
    `Build ${version ?? '?'} (${a.version}) ${build.id}: uploaded ${a.uploadedDate ?? '?'}, processing ${a.processingState ?? '?'}, expired ${a.expired ?? '?'}, export compliance ${describeEncryption(a.usesNonExemptEncryption)}`
  );
  log(
    `  internal ${detail.internalBuildState ?? '?'}, external ${detail.externalBuildState ?? '?'}, Beta App Review ${submission?.attributes?.betaReviewState ?? 'not submitted'}, What to Test (${LOCALE}) ${localization?.attributes?.whatsNew ? `${localization.attributes.whatsNew.length} chars` : 'empty'}, groups: ${groups.length ? groups.join(', ') : 'none'}`
  );
}

async function listMode(client, options) {
  const app = await findApp(client);

  const reviewDetail = await client.request('GET', `/v1/apps/${app.id}/betaAppReviewDetail`);
  const r = reviewDetail?.data?.attributes ?? {};
  const set = (value) => (value ? 'set' : 'missing');
  log(
    `Beta App Review details: contact name ${set(r.contactFirstName && r.contactLastName)}, email ${set(r.contactEmail)}, phone ${set(r.contactPhone)}, demo account required ${r.demoAccountRequired ?? '?'}`
  );
  const { data: appLocalizations } = await client.list(`/v1/apps/${app.id}/betaAppLocalizations?limit=50`);
  for (const localization of appLocalizations) {
    const l = localization.attributes ?? {};
    log(
      `Beta app information (${l.locale}): description ${set(l.description)}, feedback email ${set(l.feedbackEmail)}`
    );
  }
  if (!appLocalizations.length) log('Beta app information: none');

  const groups = await listGroups(client, app.id);
  const groupsById = new Map();
  log(`Beta groups (${groups.length}):`);
  for (const group of groups) {
    const g = group.attributes ?? {};
    const buildIds = await groupBuildIds(client, group.id);
    groupsById.set(group.id, { name: g.name, buildIds });
    log(
      `  "${g.name}" ${group.id}: ${g.isInternalGroup ? 'internal' : 'external'}, public link ${g.publicLinkEnabled ? 'on' : 'off'}, ${buildIds.size} build(s), created ${g.createdDate ?? '?'}`
    );
  }

  if (options.version) {
    const buildNumber = options.build ?? buildNumberFor(options.version);
    const build = await findBuild(client, app.id, options.version, buildNumber);
    if (!build) log(`Build ${options.version} (${buildNumber}) is not listed.`);
    else await describeBuild(client, build, options.version, groupsById);
    return;
  }
  const { data, included } = await client.request(
    'GET',
    `/v1/builds?filter[app]=${app.id}&sort=-uploadedDate&include=preReleaseVersion&limit=${RECENT_BUILDS}`
  );
  log(`Latest ${data.length} build(s):`);
  for (const build of data) {
    await describeBuild(client, build, preReleaseVersionOf(build, included ?? []), groupsById);
  }
}

async function waitForValidBuild(client, appId, version, buildNumber, timeoutMinutes, dryRun) {
  const deadline = Date.now() + timeoutMinutes * 60_000;
  let lastState = null;
  for (;;) {
    const build = await findBuild(client, appId, version, buildNumber);
    const state = build ? build.attributes?.processingState : 'NOT_LISTED';
    if (state !== lastState) {
      log(`Build ${version} (${buildNumber}) processing state: ${state}`);
      lastState = state;
    }
    if (state === 'VALID') return build;
    if (state === 'FAILED' || state === 'INVALID')
      throw new Error(`build ${version} (${buildNumber}) processing ended ${state}`);
    if (dryRun) {
      log('Dry run: not waiting for processing.');
      return build;
    }
    if (Date.now() >= deadline)
      throw new Error(
        `build ${version} (${buildNumber}) was not VALID after ${timeoutMinutes} minutes (last state ${state})`
      );
    await sleep(POLL_SECONDS * 1000);
  }
}

async function distributeMode(client, options) {
  const version = options.version;
  const buildNumber = options.build ?? buildNumberFor(version);
  if (!options.group) throw new Error('--group is required (run `list` to see the exact names)');
  const whatToTest = (process.env.GHOSTEX_WHAT_TO_TEST ?? '').trim();
  if (!whatToTest) throw new Error('GHOSTEX_WHAT_TO_TEST is empty; external testing needs a What to Test text');
  if (whatToTest.length > WHAT_TO_TEST_MAX)
    throw new Error(`What to Test is ${whatToTest.length} characters; App Store Connect allows ${WHAT_TO_TEST_MAX}`);
  const timeoutMinutes = Number(options['timeout-minutes'] ?? 90);
  if (!(timeoutMinutes > 0)) throw new Error(`--timeout-minutes must be a positive number`);
  const dryRun = options['dry-run'] === true;
  if (dryRun) log('Dry run: nothing is changed in App Store Connect.');

  const app = await findApp(client);
  const groups = await listGroups(client, app.id);
  const group = groups.find((candidate) => candidate.attributes?.name === options.group);
  if (!group)
    throw new Error(
      `no beta group named "${options.group}"; groups: ${groups.map((g) => `"${g.attributes?.name}"`).join(', ') || 'none'}`
    );
  if (group.attributes?.isInternalGroup)
    throw new Error(`beta group "${options.group}" is internal; distribution here is for external testers`);
  log(`External group: "${group.attributes.name}" (${group.id})`);

  // 1. Processing.
  const build = await waitForValidBuild(client, app.id, version, buildNumber, timeoutMinutes, dryRun);
  if (!build) {
    log(`Dry run: build ${version} (${buildNumber}) is not listed yet; the real run would wait for it.`);
    return;
  }
  const attributes = build.attributes ?? {};
  if (attributes.expired) throw new Error(`build ${version} (${buildNumber}) has expired`);
  log(`Export compliance: ${describeEncryption(attributes.usesNonExemptEncryption)}`);
  // Only a dry run reaches here with a build still processing, whose answer may not be recorded yet.
  if (attributes.processingState === 'VALID' && attributes.usesNonExemptEncryption !== false)
    throw new Error(
      `build ${version} (${buildNumber}) export compliance is ${describeEncryption(attributes.usesNonExemptEncryption)}; expected the Info.plist answer (no non-exempt encryption). Answer it in App Store Connect first.`
    );
  // A rejected build needs a person, so it stops before anything is changed.
  if ((await reviewSubmission(client, build.id))?.attributes?.betaReviewState === 'REJECTED')
    throw new Error(
      `Beta App Review rejected build ${version} (${buildNumber}); read Apple's notes in App Store Connect before resubmitting`
    );

  // 2. What to Test.
  const localization = await buildLocalization(client, build.id);
  if (localization?.attributes?.whatsNew === whatToTest) {
    log(`What to Test (${LOCALE}): already set (${whatToTest.length} characters)`);
  } else if (dryRun) {
    log(
      `Dry run: would ${localization ? 'update' : 'create'} What to Test (${LOCALE}, ${whatToTest.length} characters)`
    );
  } else if (localization) {
    await client.request('PATCH', `/v1/betaBuildLocalizations/${localization.id}`, {
      data: {
        type: 'betaBuildLocalizations',
        id: localization.id,
        attributes: { whatsNew: whatToTest },
      },
    });
    log(`What to Test (${LOCALE}): updated (${whatToTest.length} characters)`);
  } else {
    await client.request('POST', '/v1/betaBuildLocalizations', {
      data: {
        type: 'betaBuildLocalizations',
        attributes: { locale: LOCALE, whatsNew: whatToTest },
        relationships: { build: { data: { type: 'builds', id: build.id } } },
      },
    });
    log(`What to Test (${LOCALE}): created (${whatToTest.length} characters)`);
  }

  // 3. External group.
  const members = await groupBuildIds(client, group.id);
  if (members.has(build.id)) {
    log(`Group "${group.attributes.name}": build already added`);
  } else if (dryRun) {
    log(`Dry run: would add the build to "${group.attributes.name}"`);
  } else {
    await client.request('POST', `/v1/betaGroups/${group.id}/relationships/builds`, {
      data: [{ type: 'builds', id: build.id }],
    });
    log(`Group "${group.attributes.name}": build added`);
  }

  // 4. Beta App Review.
  const detail = await betaDetail(client, build.id);
  const submission = await reviewSubmission(client, build.id);
  const reviewState = submission?.attributes?.betaReviewState ?? null;
  log(`Beta App Review: submission ${reviewState ?? 'none'}, external state ${detail.externalBuildState ?? '?'}`);
  if (reviewState === 'REJECTED')
    throw new Error(
      `Beta App Review rejected build ${version} (${buildNumber}); read Apple's notes in App Store Connect before resubmitting`
    );
  if (REVIEW_PENDING_OR_DONE.has(reviewState) || EXTERNAL_SUBMITTED_OR_DONE.has(detail.externalBuildState)) {
    log('Beta App Review: nothing to submit');
  } else if (dryRun) {
    log('Dry run: would submit the build for Beta App Review');
  } else {
    const created = await client.request('POST', '/v1/betaAppReviewSubmissions', {
      data: {
        type: 'betaAppReviewSubmissions',
        relationships: { build: { data: { type: 'builds', id: build.id } } },
      },
    });
    log(`Beta App Review: submitted (${created?.data?.attributes?.betaReviewState ?? 'state unknown'})`);
  }

  if (!dryRun) {
    const after = await betaDetail(client, build.id);
    log(`Done: Ghostex ${version} (${buildNumber}) external state ${after.externalBuildState ?? '?'}`);
  }
}

async function main(argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      version: { type: 'string' },
      build: { type: 'string' },
      group: { type: 'string' },
      'timeout-minutes': { type: 'string' },
      'dry-run': { type: 'boolean' },
    },
  });
  if (values.version !== undefined) buildNumberFor(values.version);
  if (values.build !== undefined && !/^\d+$/.test(values.build))
    throw new Error(`--build must be a number, got ${values.build}`);
  const mode = positionals[0];
  const client = ascClientFromEnv();
  if (mode === 'list') return listMode(client, values);
  if (mode === 'distribute') {
    if (!values.version) throw new Error('--version is required');
    return distributeMode(client, values);
  }
  throw new Error('usage: asc-testflight-distribute.mjs list|distribute [options] (see the header)');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
