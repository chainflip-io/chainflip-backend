import fs from 'fs';
import { jsonRpc } from '../json_rpc';

type RuntimeVersion = {
  specName: string;
  implName: string;
  authoringVersion: number;
  specVersion: number;
  implVersion: number;
  apis: [string, number][];
  transactionVersion: number;
  stateVersion: number;
};

export async function getNetworkRuntimeVersion(endpoint?: string): Promise<RuntimeVersion> {
  return (await jsonRpc('state_getRuntimeVersion', [], endpoint)) as unknown as RuntimeVersion;
}

// If `onlyReadCurrent` is true, it will only read the current spec version and return it.
// If `onlyReadCurrent` is false, it will increment the spec version and write it to the file. Returning the newly written version.
export function specVersion(
  filePath: string,
  readOrWrite: 'read' | 'write',
  writeSpecVersion?: number,
): number {
  try {
    const fileContent = fs.readFileSync(filePath, 'utf-8');
    const lines = fileContent.split('\n');

    let incrementedVersion = -1;
    let foundMacro = false;
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];

      if (line.trim() === '#[sp_version::runtime_version]') {
        foundMacro = true;
      }

      if (foundMacro && line.includes('spec_version:')) {
        const specVersionLine = line.match(/(spec_version:\s*)(\d+)/);

        if (specVersionLine) {
          const currentSpecVersion = parseInt(specVersionLine[2]);

          if (readOrWrite === 'read') {
            return currentSpecVersion;
          }

          if (writeSpecVersion) {
            incrementedVersion = writeSpecVersion;
          } else {
            incrementedVersion = currentSpecVersion + 1;
          }
          lines[i] = `	spec_version: ${incrementedVersion},`;
          break;
        }
      }
    }

    if (!foundMacro) {
      console.error('spec_version within #[sp_version::runtime_version] not found.');
      return -1;
    }

    const updatedContent = lines.join('\n');
    fs.writeFileSync(filePath, updatedContent);

    console.log(`Successfully updated spec_version to ${incrementedVersion}.`);
    return incrementedVersion;
  } catch (error) {
    console.error(`An error occurred: ${error.message}`);
    return -1;
  }
}

// Bump the spec version in the runtime file, using the spec version of the network.
export async function bumpSpecVersionAgainstNetwork(
  runtimeLibPath: string,
  endpoint?: string,
): Promise<number> {
  const networkSpecVersion = (await getNetworkRuntimeVersion(endpoint)).specVersion;
  console.log('Current spec_version: ' + networkSpecVersion);
  const nextSpecVersion = networkSpecVersion + 1;
  console.log('Bumping the spec version to: ' + nextSpecVersion);
  specVersion(runtimeLibPath, 'write', nextSpecVersion);
  return nextSpecVersion;
}
