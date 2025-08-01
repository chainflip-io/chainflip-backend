import fs from 'fs';
import { jsonRpc } from 'shared/json_rpc';
import { Logger } from 'shared/utils/logger';

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

export async function getNetworkRuntimeVersion(
  logger: Logger,
  endpoint?: string,
): Promise<RuntimeVersion> {
  return (await jsonRpc(
    logger,
    'state_getRuntimeVersion',
    [],
    endpoint,
  )) as unknown as RuntimeVersion;
}

export function specVersion(
  logger: Logger,
  filePath: string,
  readOrWrite: 'read' | 'write',
  // Will only write this version if the current version is less than this.
  // If this is not provided it will simply bump the version in the file by 1.
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
        const specVersionLine = line.match(/(spec_version:\s*)(\d+(?:_\d+)*)/);

        if (specVersionLine) {
          const versionWithUnderscores = specVersionLine[2];
          const currentSpecVersion = parseInt(versionWithUnderscores.replace(/_/g, ''));

          if (readOrWrite === 'read') {
            return currentSpecVersion;
          }
          // write

          if (writeSpecVersion) {
            if (currentSpecVersion >= writeSpecVersion) {
              logger.info(
                "Current spec version is greater than the one you're trying to write. Returning currentSpecVersion.",
              );
              return currentSpecVersion;
            }
            // if the version we provided is greater than the current one, then we can bump it to this new version.
            incrementedVersion = writeSpecVersion;
          } else {
            // If we want to write, but didn't provide a version, we simply increment the current version.
            incrementedVersion = currentSpecVersion + 1;
          }

          console.assert(
            incrementedVersion !== -1,
            'incrementedVersion should not be -1. It should be set above.',
          );
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

    logger.info(`Successfully updated spec_version to ${incrementedVersion}.`);
    return incrementedVersion;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } catch (error: any) {
    console.error(`An error occurred: ${error.message}`);
    return -1;
  }
}

// Bump the spec version in the runtime file, using the spec version of the network.
export async function bumpSpecVersionAgainstNetwork(
  logger: Logger,
  runtimeLibPath: string,
  endpoint?: string,
): Promise<number> {
  const networkSpecVersion = (await getNetworkRuntimeVersion(logger, endpoint)).specVersion;
  logger.debug('Current spec_version: ' + networkSpecVersion);
  const nextSpecVersion = networkSpecVersion + 1;
  logger.info('Bumping the spec version to: ' + nextSpecVersion);
  specVersion(logger, runtimeLibPath, 'write', nextSpecVersion);
  return nextSpecVersion;
}
