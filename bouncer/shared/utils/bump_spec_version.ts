import fs from 'fs';
import { jsonRpc } from '../json_rpc';

export async function getCurrentSpecVersion(): Promise<number> {
  return Number((await jsonRpc('state_getRuntimeVersion', [], 9944)).specVersion);
}

export function bumpSpecVersion(filePath: string, nextSpecVersion?: number) {
  try {
    const fileContent = fs.readFileSync(filePath, 'utf-8');
    const lines = fileContent.split('\n');

    let incrementedVersion;
    let foundMacro = false;
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];

      if (line.trim() === '#[sp_version::runtime_version]') {
        foundMacro = true;
      }

      if (foundMacro && line.includes('spec_version:')) {
        const specVersionLine = line.match(/(spec_version:\s*)(\d+)/);

        if (specVersionLine) {
          if (nextSpecVersion) {
            incrementedVersion = nextSpecVersion;
          } else {
            incrementedVersion = parseInt(specVersionLine[2]) + 1;
          }
          lines[i] = `	spec_version: ${incrementedVersion},`;
          break;
        }
      }
    }

    if (!foundMacro) {
      console.error('spec_version within #[sp_version::runtime_version] not found.');
      return;
    }

    const updatedContent = lines.join('\n');
    fs.writeFileSync(filePath, updatedContent);

    console.log(`Successfully updated spec_version to ${incrementedVersion}.`);
  } catch (error) {
    console.error(`An error occurred: ${error.message}`);
  }
}

// Bump the spec version in the runtime file, using the spec version of the network.
export async function bumpSpecVersionAgainstNetwork(projectRoot: string): Promise<number> {
  const currentSpecVersion = await getCurrentSpecVersion();
  console.log('Current spec_version: ' + currentSpecVersion);
  const nextSpecVersion = currentSpecVersion + 1;
  console.log('Bumping the spec version to: ' + nextSpecVersion);
  bumpSpecVersion(`${projectRoot}/state-chain/runtime/src/lib.rs`, nextSpecVersion);
  return nextSpecVersion;
}
