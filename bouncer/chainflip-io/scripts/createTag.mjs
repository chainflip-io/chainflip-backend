#!/usr/bin/env node --trace-uncaught

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

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));

const root = path.join(__dirname, '../');

const packages = await fs.readdir(path.join(root, 'packages'));

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
    choices: packages,
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
  .parse();

const onMain =
  (await execAsync('git branch --show-current')).stdout.trim() === 'main';

if (!onMain) {
  console.error('please switch to branch "main"');
  process.exit(1);
}

try {
  await execAsync('git pull origin main --ff-only');
} catch {
  console.error(
    'failed to pull latest changes from main, perhaps your branch has diverged?',
  );
  process.exit(1);
}

let isDryRun = args['dry-run'];

if (isDryRun) console.log('DRY RUN MODE');

const execCommand = async (cmd) => {
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

let newVersion = args['new-version'];
const packageRoot = path.join(root, 'packages', args.package);
const packageJSON = JSON.parse(
  await fs.readFile(path.join(packageRoot, 'package.json'), 'utf-8'),
);

if (!newVersion) {
  const currentVersion = packageJSON.version;

  if (typeof currentVersion !== 'string') {
    console.error('failed to find current version');
    process.exit(1);
  }

  const [major, minor, patch] = currentVersion.split('.');

  if (args.minor) {
    newVersion = `${major}.${Number(minor) + 1}.0`;
  } else if (args.major) {
    newVersion = `${Number(major) + 1}.0.0`;
  } else {
    newVersion = `${major}.${minor}.${Number(patch) + 1}`;
  }
}

const tagPkg = async () => {
  await execCommand(
    `pnpm --filter ${packageJSON.name} exec pnpm version ${newVersion}`,
  );
  const tag = `${packageJSON.name}/v${newVersion}`;
  await execCommand(`git commit -a -m "${tag}" --no-verify`);
  await execCommand(`git tag ${tag}`);
  await execCommand('git push');
  await execCommand(`git push origin refs/tags/${tag}`);
};

await tagPkg();

if (isDryRun) {
  console.log('END DRY RUN MODE');
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const questionAsync = util.promisify(rl.question).bind(rl);

  const runAgain = await questionAsync(
    'would you like to run again without dry run?\n(y/N)> ',
  );

  rl.close();

  if (runAgain?.trim().toLowerCase() === 'y') {
    isDryRun = false;
    console.log('running without dry run mode');
    await tagPkg();
  }
}
