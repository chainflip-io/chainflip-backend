import { execSync } from 'node:child_process';

export function isNetworkConnected(containerName: string, networkName: string): boolean {
  const res = execSync(`docker inspect ${containerName}`);
  return JSON.parse(res.toString())[0].NetworkSettings.Networks[networkName] !== undefined;
}

export async function disconnectContainerFromNetwork(containerName: string, networkName: string) {
  execSync(`docker network disconnect ${networkName} ${containerName}`);
  if (isNetworkConnected(containerName, networkName)) {
    throw new Error('Failed to disconnect container from network');
  }
  console.log(`Disconnected ${containerName} from ${networkName}!`);
}

export async function connectContainerToNetwork(containerName: string, networkName: string) {
  execSync(`docker network connect ${networkName} ${containerName}`);
  if (!isNetworkConnected(containerName, networkName)) {
    throw new Error('Failed to connect container to network');
  }
  console.log(`Connected ${containerName} to ${networkName}!`);
}
