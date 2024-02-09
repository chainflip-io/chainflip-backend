import { ArgumentsCamelCase, InferredOptionTypes, Options } from 'yargs';
import { Assets, Chains } from '@/shared/enums';
import { BrokerClient } from '../lib';

export const yargsOptions = {
  'src-asset': {
    choices: Object.values(Assets),
    describe: 'The asset to swap from',
    demandOption: true,
  },
  'dest-asset': {
    choices: Object.values(Assets),
    demandOption: true,
    describe: 'The asset to swap to',
  },
  'dest-address': {
    type: 'string',
    demandOption: true,
    describe: 'The address to send the swapped assets to',
  },
  'broker-url': {
    type: 'string',
    describe: 'The broker URL',
    demandOption: true,
  },
  'src-chain': {
    choices: Object.values(Chains),
    describe: 'The chain to swap from',
    demandOption: true,
  },
  'dest-chain': {
    choices: Object.values(Chains),
    describe: 'The chain to swap to',
    demandOption: true,
  },
} as const satisfies { [key: string]: Options };

export default async function cliRequestSwapDepositAddress(
  args: ArgumentsCamelCase<InferredOptionTypes<typeof yargsOptions>>,
) {
  const client = await BrokerClient.create({ url: args.brokerUrl });

  const result = await client.requestSwapDepositAddress({
    srcAsset: args.srcAsset,
    destAsset: args.destAsset,
    destAddress: args.destAddress,
    srcChain: args.srcChain,
    destChain: args.destChain,
  });

  console.log(`Deposit address: ${result.address}`);
  console.log(`Issued block: ${result.issuedBlock}`);
  console.log(`Expiry block: ${result.expiryBlock}`);
  console.log(`Channel ID: ${result.channelId}`);

  await client.close();
}
