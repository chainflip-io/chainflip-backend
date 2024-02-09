#!/usr/bin/env node --import=tsx --trace-uncaught --no-warnings

/* eslint-disable no-console */
import { exec } from 'child_process';
import * as fs from 'fs/promises';
import * as path from 'path';
import { createInterface } from 'readline';
import * as url from 'url';
import * as util from 'util';
// eslint-disable-next-line import/no-extraneous-dependencies
import yargs from 'yargs/yargs';

const execAsync = util.promisify(exec);

// @ts-expect-error -- .mts file
const __dirname = path.dirname(url.fileURLToPath(import.meta.url));

const root = path.join(__dirname, '../');

const args = yargs(process.argv)
  .option('new-version', {
    type: 'string',
    description: 'escape hatch for specifying the new version',
  })
  .option('minor', {
    description: 'Increment minor version',
    boolean: true,
  })
  .option('major', {
    description: 'Increment major version',
    boolean: true,
  })
  .option('package', {
    alias: 'p',
    description: 'the package to tag',
    demandOption: true,
    choices: ['cli', 'sdk'],
  })
  .option('dry-run', {
    demandOption: false,
    default: !['0', 'false'].includes(process.env.DRY_RUN?.toLowerCase()),
    boolean: true,
    description:
      'whether the script should run in dry run mode, can be disabled with `DRY_RUN=false` or `--no-dry-run`. ' +
      'additionally, there is a prompt after dry run mode to run the script live',
  })
  .help()
  .parseSync();

const onMain =
  // @ts-expect-error -- .mts file
  (await execAsync('git branch --show-current')).stdout.trim() === 'main';

if (!onMain) {
  console.error('please switch to branch "main"');
  process.exit(1);
}

// const workingDirectoryDirty =
//   // @ts-expect-error -- .mts file
//   (await execAsync('git status --porcelain=v2')).stdout
//     .trim()
//     .split('\n')
//     .filter(Boolean)
//     .filter((line) => !line.startsWith('?')).length !== 0;

// if (workingDirectoryDirty) {
//   console.error(
//     'working directory is dirty, please stash changes before proceeding',
//   );
//   process.exit(1);
// }

try {
  // @ts-expect-error -- .mts file
  await execAsync('git pull origin main --ff-only');
} catch {
  console.error(
    'failed to pull latest changes from main, perhaps your branch has diverged?',
  );
  process.exit(1);
}

let newVersion = args['new-version'];
const packageRoot = path.join(root, 'packages', args.package);
const packageJSON = JSON.parse(
  // @ts-expect-error -- .mts file
  await fs.readFile(path.join(packageRoot, 'package.json'), 'utf-8'),
);

if (!newVersion) {
  const currentVersion = packageJSON.version;

  if (typeof currentVersion !== 'string') {
    console.error('failed to find current version');
    process.exit(1);
  }

  const [, major, minor, patch] = /^(\d+)\.(\d+)\.(\d+)/.exec(currentVersion);

  if (args.minor) {
    newVersion = `${major}.${Number(minor) + 1}.0`;
  } else if (args.major) {
    newVersion = `${Number(major) + 1}.0.0`;
  } else {
    newVersion = `${major}.${minor}.${Number(patch) + 1}`;
  }
}

let isDryRun = args['dry-run'];

if (isDryRun) console.log('DRY RUN MODE');

const execCommand = async (cmd: string) => {
  console.log('executing command %O', cmd);

  if (!isDryRun) {
    try {
      await execAsync(cmd);
    } catch (error) {
      console.error(error);
      process.exit(1);
    }
  }
};

const openVersionPR = async () => {
  const message = `chore(${args.package}): release ${packageJSON.name}/v${newVersion}`;
  const newBranch = `chore/release-${newVersion}`;
  await execCommand(`git switch -c ${newBranch}`);
  await execCommand(
    `pnpm --filter ${packageJSON.name} exec pnpm version ${newVersion}`,
  );
  await execCommand(`git add .`);
  await execCommand(`git commit -m "${message}" --no-verify`);
  await execCommand(`git push origin ${newBranch}`);
  await execCommand(`gh pr create --title "${message}" --body ""`);
};

// @ts-expect-error -- .mts file
await openVersionPR();

if (isDryRun) {
  console.log('END DRY RUN MODE');
  const rl = createInterface({ input: process.stdin, output: process.stdout });

  const questionAsync = util.promisify(rl.question).bind(rl);

  // @ts-expect-error -- .mts file
  const runAgain = await questionAsync(
    'would you like to run again without dry run?\n(y/N)> ',
  );

  rl.close();

  if (runAgain?.trim().toLowerCase() === 'y') {
    isDryRun = false;
    console.log('running without dry run mode');
    // @ts-expect-error -- .mts file
    await openVersionPR();
  }
}
