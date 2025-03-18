import { execSync } from 'node:child_process';
import { Logger } from './utils/logger';

export function isNetworkConnected(containerName: string, networkName: string): boolean {
  const res = execSync(`docker inspect ${containerName}`);
  return JSON.parse(res.toString())[0].NetworkSettings.Networks[networkName] !== undefined;
}

export async function disconnectContainerFromNetwork(
  logger: Logger,
  containerName: string,
  networkName: string,
) {
  execSync(`docker network disconnect ${networkName} ${containerName}`);
  if (isNetworkConnected(containerName, networkName)) {
    throw new Error('Failed to disconnect container from network');
  }
  logger.info(`Disconnected ${containerName} from ${networkName}!`);
}

export async function connectContainerToNetwork(
  logger: Logger,
  containerName: string,
  networkName: string,
) {
  execSync(`docker network connect ${networkName} ${containerName}`);
  if (!isNetworkConnected(containerName, networkName)) {
    throw new Error('Failed to connect container to network');
  }
  logger.info(`Connected ${containerName} to ${networkName}!`);
}
