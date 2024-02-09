#!/usr/bin/env node --import=tsx --trace-uncaught --no-warnings

/* eslint-disable no-console, import/no-extraneous-dependencies */
import assert from 'assert';
import { exec } from 'child_process';
import { createInterface } from 'readline/promises';
import * as util from 'util';
import yargs from 'yargs/yargs';

const execAsync = util.promisify(exec);

const ask = async (q: string) => {
  const rl = createInterface({ input: process.stdin, output: process.stdout });

  try {
    const answer = await rl.question(q);

    return answer.trim();
  } finally {
    rl.close();
  }
};

const args = yargs(process.argv)
  .option('package', {
    demandOption: true,
    type: 'string',
    choices: ['swap'] as const,
    description: 'the package we want to tag',
  })
  .option('new-version', {
    type: 'string',
    description: 'escape hatch for specifying the new version',
  })
  .option('dry-run', {
    demandOption: false,
    default: true,
    boolean: true,
    description:
      'whether the script should run in dry run mode, can be disabled with `--no-dry-run`. ' +
      'additionally, there is a prompt after dry run mode to run the script live',
  })
  .help()
  .parseSync();

const currentBranch = // @ts-expect-error -- .mts file
  (await execAsync('git branch --show-current')).stdout.trim();

const releaseVersion = /^release\/(\d\.\d)/.exec(currentBranch)?.[1];

if (!releaseVersion) {
  console.error('please switch to a release branch');
  process.exit(1);
}

// @ts-expect-error -- .mts file
const { stdout: lastTag } = await execAsync(
  `git tag | grep "${args.package}/v${releaseVersion}" | sort -V | tail -n 1`,
);

let patch: string;
if (lastTag === '') {
  patch = '0';
} else {
  const match = new RegExp(
    String.raw`${args.package}/v${releaseVersion}\.(\d+)`,
  ).exec(lastTag)?.[1];

  assert(match, 'could not find last tag');

  patch = String(Number(match) + 1);
}

let { dryRun } = args;

if (dryRun) console.log('DRY RUN MODE');

const execCommand = async (cmd: string) => {
  console.log('executing command %O', cmd);

  if (!dryRun) {
    try {
      await execAsync(cmd);
    } catch (error) {
      console.error(error);
      process.exit(1);
    }
  }
};

const tagPackage = async () => {
  const newTag = `${args.package}/v${releaseVersion}.${patch}`;
  await execCommand(`git tag ${newTag}`);
  await execCommand(`git push origin refs/tags/${newTag}`);
};

// @ts-expect-error -- .mts file
await tagPackage();

if (dryRun) {
  console.log('END DRY RUN MODE');

  // @ts-expect-error -- .mts file
  const runAgain = await ask(
    'would you like to run again without dry run?\n(y/N)> ',
  );

  if (runAgain.toLowerCase() === 'y') {
    dryRun = false;
    console.log('running without dry run mode');
    // @ts-expect-error -- .mts file
    await tagPackage();
  }
}
