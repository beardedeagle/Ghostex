#!/usr/bin/env node
/*
 * Poll one Actions run and print only what changed.
 *
 * CDXC:Release 2026-09-16 WHY: the release operator watches a run from inside
 * an agent context, where every printed line costs tokens, so `gh run watch`
 * (which redraws the whole job table every few seconds) is the wrong tool.
 * This prints one line per job transition, one summary line per change, and
 * nothing at all while nothing moves. Exit codes let a script branch on the
 * outcome without parsing the text.
 *
 * Usage:
 *   node tooling/release-gpui/watch-run.mjs --run <run-id> [--interval 300]
 *     [--repo maddada/Ghostex] [--max-minutes 180] [--once] [--exit-on-change]
 *
 * Exit codes: 0 run finished, 1 a job failed, 2 --max-minutes elapsed, 3 gh
 * failed five polls in a row.
 *
 * --exit-on-change also exits 0 as soon as any job's status or conclusion
 * differs from the previous poll, after printing the change lines: an operator
 * that runs the watcher as a background process is woken exactly once per
 * state change and starts a new watcher. The first poll only records the
 * baseline (and lists what is still moving or broken); a terminal run still
 * prints the go-live summary and exits with the normal code, so
 * `run <run-id> completed:` on stdout distinguishes "over" from "changed".
 */

import { execFile } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const MAX_CONSECUTIVE_POLL_FAILURES = 5;

function usage() {
  return (
    'Usage: node tooling/release-gpui/watch-run.mjs --run <run-id> [--interval 300] ' +
    '[--repo maddada/Ghostex] [--max-minutes 180] [--once] [--exit-on-change]'
  );
}

export function parseArgs(argv) {
  const options = {
    exitOnChange: false,
    interval: 300,
    maxMinutes: 180,
    once: false,
    repo: 'maddada/Ghostex',
    runId: null,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      const value = argv[index + 1];
      if (value === undefined) throw new Error(`${arg} requires a value`);
      index += 1;
      return value;
    };
    if (arg === '--help' || arg === '-h') return { ...options, help: true };
    if (arg === '--once') options.once = true;
    else if (arg === '--exit-on-change') options.exitOnChange = true;
    else if (arg === '--run') {
      const match = /(\d+)\/?$/u.exec(next());
      if (!match) throw new Error('--run requires a numeric run id (or a run URL ending in one)');
      options.runId = match[1];
    } else if (arg === '--interval') options.interval = Number(next());
    else if (arg === '--max-minutes') options.maxMinutes = Number(next());
    else if (arg === '--repo') options.repo = next();
    else throw new Error(`Unknown option: ${arg}`);
  }
  if (!options.runId) throw new Error(usage());
  if (!Number.isFinite(options.interval) || options.interval < 1) throw new Error('--interval must be >= 1 second');
  if (!Number.isFinite(options.maxMinutes) || options.maxMinutes <= 0) throw new Error('--max-minutes must be > 0');
  return options;
}

const FAILED_CONCLUSIONS = new Set(['failure', 'timed_out', 'cancelled', 'action_required', 'startup_failure']);

export function isFailedJob(job) {
  return job.status === 'completed' && FAILED_CONCLUSIONS.has(job.conclusion);
}

export function summarize(jobs) {
  const counts = { failed: 0, queued: 0, running: 0, skipped: 0, succeeded: 0 };
  for (const job of jobs) {
    if (job.status === 'in_progress') counts.running += 1;
    else if (job.status !== 'completed') counts.queued += 1;
    else if (job.conclusion === 'skipped') counts.skipped += 1;
    else if (isFailedJob(job)) counts.failed += 1;
    else counts.succeeded += 1;
  }
  return counts;
}

export function formatSummary(counts) {
  return (
    `running ${counts.running} / queued ${counts.queued} / succeeded ${counts.succeeded} / ` +
    `failed ${counts.failed} / skipped ${counts.skipped}`
  );
}

function clock(iso) {
  const date = iso ? new Date(iso) : new Date();
  return `${date.toISOString().slice(11, 19)}Z`;
}

function jobStamp(job) {
  return clock(job.status === 'completed' ? job.completedAt : job.startedAt);
}

export function formatJobLine(job) {
  const state = job.status === 'completed' ? `completed  ${job.conclusion ?? 'unknown'}` : job.status;
  return `${jobStamp(job)}  ${job.name}  ${state}`;
}

function stateKey(job) {
  return `${job.status}|${job.conclusion ?? ''}`;
}

/* Jobs whose state differs from the last poll; every job counts on the first poll. */
export function changedJobs(previous, jobs) {
  if (!previous) return jobs;
  return jobs.filter((job) => previous.get(job.databaseId) !== stateKey(job));
}

/* The go-live moment of every stage: `publish_<stage> / publish` succeeded. */
export function goLiveLines(jobs) {
  return jobs
    .filter(
      (job) => /^publish_.+ \/ publish$/u.test(job.name) && job.status === 'completed' && job.conclusion === 'success'
    )
    .map((job) => `${job.name}  live at ${job.completedAt}`);
}

async function pollRun({ repo, runId }) {
  const { stdout } = await execFileAsync(
    'gh',
    ['run', 'view', runId, '--repo', repo, '--json', 'status,conclusion,jobs'],
    { maxBuffer: 16 * 1024 * 1024 }
  );
  const run = JSON.parse(stdout);
  return { conclusion: run.conclusion ?? null, jobs: run.jobs ?? [], status: run.status ?? 'unknown' };
}

function sleep(seconds) {
  return new Promise((resolve) => setTimeout(resolve, seconds * 1000));
}

/*
 * One observation of the run. Prints the changed jobs and the summary, and
 * returns an exit code when the watch is over (a fatal job failure, a
 * terminal run status, or with exitOnChange any change after the baseline
 * poll); null means keep polling.
 */
export function observe({ exitOnChange = false, previous, run, runId }, print = console.log) {
  const changed = changedJobs(previous, run.jobs);
  const baseline = previous === null;
  const lines = changed
    /* The first poll lists what is still moving or already broken; finished-green history is noise. */
    .filter((job) => !baseline || !(job.status === 'completed' && ['success', 'skipped'].includes(job.conclusion)))
    .map(formatJobLine);
  if (changed.length > 0) {
    for (const line of lines) print(line);
    print(`${clock()}  ${formatSummary(summarize(run.jobs))}`);
  }
  const failed = run.jobs.filter(isFailedJob);
  if (failed.length > 0) {
    const job = failed[0];
    print(`FAIL  ${job.name}  job ${job.databaseId}  (gh run view --job ${job.databaseId} --log-failed)`);
    return 1;
  }
  if (run.status === 'completed') {
    print(`run ${runId} completed: ${run.conclusion ?? 'unknown'}`);
    for (const line of goLiveLines(run.jobs)) print(line);
    return 0;
  }
  if (exitOnChange && !baseline && changed.length > 0) {
    print(`${clock()}  run ${runId} changed; exiting (--exit-on-change), start a new watcher`);
    return 0;
  }
  return null;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage());
    return 0;
  }
  const deadline = Date.now() + options.maxMinutes * 60 * 1000;
  let previous = null;
  let consecutiveFailures = 0;
  for (;;) {
    let run;
    try {
      run = await pollRun(options);
      consecutiveFailures = 0;
    } catch (error) {
      if (error?.code === 'ENOENT') {
        console.error('gh is not installed or not on PATH');
        return 3;
      }
      consecutiveFailures += 1;
      if (consecutiveFailures === 1) {
        const reason = String(error?.stderr || error?.message || error)
          .trim()
          .split('\n')[0];
        console.error(`${clock()}  poll failed (${reason})${options.once ? '' : '; retrying'}`);
      }
      if (options.once) return 3;
      if (consecutiveFailures >= MAX_CONSECUTIVE_POLL_FAILURES) {
        console.error(`${clock()}  ${MAX_CONSECUTIVE_POLL_FAILURES} consecutive poll failures; giving up`);
        return 3;
      }
      await sleep(Math.min(options.interval, 30));
      continue;
    }
    const exitCode = observe({ exitOnChange: options.exitOnChange, previous, run, runId: options.runId });
    if (exitCode !== null) return exitCode;
    if (options.once) {
      console.log(`run ${options.runId} ${run.status}`);
      return 0;
    }
    previous = new Map(run.jobs.map((job) => [job.databaseId, stateKey(job)]));
    if (Date.now() >= deadline) {
      console.log(`${clock()}  --max-minutes ${options.maxMinutes} elapsed; run ${options.runId} still ${run.status}`);
      return 2;
    }
    await sleep(options.interval);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (error) => {
      console.error(error instanceof Error ? error.message : error);
      process.exitCode = 1;
    }
  );
}
