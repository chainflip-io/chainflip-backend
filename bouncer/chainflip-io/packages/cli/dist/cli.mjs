#! /usr/bin/env node
import { fileURLToPath } from 'url';
import { createRequire } from 'module';
import * as path from 'path';
import { __name, chainflipNetwork, Assets, ChainflipNetworks, Chains, executeCall_default, assetChains, executeSwap_default, fundStateChainAccount, BrokerClient } from './chunk-6GTU7ZVU.mjs';
import yargs from 'yargs/yargs';
import { Wallet, providers, getDefaultProvider } from 'ethers';
import { createInterface } from 'readline/promises';

createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
path.dirname(__filename);
var askForPrivateKey = /* @__PURE__ */ __name(async () => {
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout
  });
  try {
    return await rl.question("Please enter your wallet's private key: ");
  } finally {
    rl.close();
  }
}, "askForPrivateKey");
function getEthNetwork(opts) {
  if (opts.chainflipNetwork === "localnet")
    return opts.ethNetwork;
  if (opts.chainflipNetwork === ChainflipNetworks.mainnet)
    return "mainnet";
  return "goerli";
}
__name(getEthNetwork, "getEthNetwork");
var cliNetworks = [
  ...Object.values(chainflipNetwork.enum),
  "localnet"
];

// src/commands/cliExecuteCall.ts
var yargsOptions = {
  "src-asset": {
    choices: Object.values(Assets),
    demandOption: true,
    describe: "The asset to swap from"
  },
  "dest-asset": {
    choices: Object.values(Assets),
    demandOption: true,
    describe: "The asset to swap to"
  },
  "chainflip-network": {
    choices: cliNetworks,
    describe: "The Chainflip network to execute the swap on",
    default: ChainflipNetworks.sisyphos
  },
  amount: {
    type: "string",
    demandOption: true,
    describe: "The amount to swap"
  },
  "dest-address": {
    type: "string",
    demandOption: true,
    describe: "The address to send the swapped assets to"
  },
  message: {
    type: "string",
    demandOption: true,
    describe: "The message that is sent along with the swapped assets"
  },
  "gas-amount": {
    type: "string",
    demandOption: true,
    describe: "The maximum gas amount that is sent with the message"
  },
  "wallet-private-key": {
    type: "string",
    describe: "The private key of the wallet to use"
  },
  "src-token-contract-address": {
    type: "string",
    describe: "The contract address of the token to swap from when `chainflip-network` is `localnet`"
  },
  "vault-contract-address": {
    type: "string",
    describe: "The contract address of the vault when `chainflip-network` is `localnet`"
  },
  "eth-network": {
    type: "string",
    describe: "The eth network URL to use when `chainflip-network` is `localnet`"
  }
};
async function cliExecuteCall(args) {
  const privateKey = args.walletPrivateKey ?? await askForPrivateKey();
  const ethNetwork = getEthNetwork(args);
  const wallet = new Wallet(privateKey).connect(process.env.ALCHEMY_KEY ? new providers.AlchemyProvider(ethNetwork, process.env.ALCHEMY_KEY) : getDefaultProvider(ethNetwork));
  const opts = args.chainflipNetwork === "localnet" ? {
    vaultContractAddress: args.vaultContractAddress,
    srcTokenContractAddress: args.srcTokenContractAddress,
    signer: wallet,
    network: args.chainflipNetwork
  } : {
    network: args.chainflipNetwork,
    signer: wallet
  };
  const receipt = await executeCall_default({
    srcChain: assetChains[args.srcAsset],
    srcAsset: args.srcAsset,
    destChain: assetChains[args.destAsset],
    destAsset: args.destAsset,
    amount: args.amount,
    destAddress: args.destAddress,
    message: args.message,
    gasAmount: args.gasAmount
  }, opts);
  console.log(`Call executed. Transaction hash: ${receipt.transactionHash}`);
}
__name(cliExecuteCall, "cliExecuteCall");
var yargsOptions2 = {
  "src-asset": {
    choices: Object.values(Assets),
    demandOption: true,
    describe: "The asset to swap from"
  },
  "dest-asset": {
    choices: Object.values(Assets),
    demandOption: true,
    describe: "The asset to swap to"
  },
  "chainflip-network": {
    choices: cliNetworks,
    describe: "The Chainflip network to execute the swap on",
    default: ChainflipNetworks.sisyphos
  },
  amount: {
    type: "string",
    demandOption: true,
    describe: "The amount to swap"
  },
  "dest-address": {
    type: "string",
    demandOption: true,
    describe: "The address to send the swapped assets to"
  },
  "wallet-private-key": {
    type: "string",
    describe: "The private key of the wallet to use"
  },
  "src-token-contract-address": {
    type: "string",
    describe: "The contract address of the token to swap from when `chainflip-network` is `localnet`"
  },
  "vault-contract-address": {
    type: "string",
    describe: "The contract address of the vault when `chainflip-network` is `localnet`"
  },
  "eth-network": {
    type: "string",
    describe: "The eth network URL to use when `chainflip-network` is `localnet`"
  }
};
async function cliExecuteSwap(args) {
  const privateKey = args.walletPrivateKey ?? await askForPrivateKey();
  const ethNetwork = getEthNetwork(args);
  const wallet = new Wallet(privateKey).connect(process.env.ALCHEMY_KEY ? new providers.AlchemyProvider(ethNetwork, process.env.ALCHEMY_KEY) : getDefaultProvider(ethNetwork));
  const opts = args.chainflipNetwork === "localnet" ? {
    vaultContractAddress: args.vaultContractAddress,
    srcTokenContractAddress: args.srcTokenContractAddress,
    signer: wallet,
    network: args.chainflipNetwork
  } : {
    network: args.chainflipNetwork,
    signer: wallet
  };
  const receipt = await executeSwap_default({
    srcChain: assetChains[args.srcAsset],
    srcAsset: args.srcAsset,
    destChain: assetChains[args.destAsset],
    destAsset: args.destAsset,
    amount: args.amount,
    destAddress: args.destAddress
  }, opts);
  console.log(`Swap executed. Transaction hash: ${receipt.transactionHash}`);
}
__name(cliExecuteSwap, "cliExecuteSwap");
var yargsOptions3 = {
  "src-account-id": {
    type: "string",
    demandOption: true,
    describe: "The account ID for the validator to be funded"
  },
  "chainflip-network": {
    choices: cliNetworks,
    describe: "The Chainflip network to execute the swap on",
    default: ChainflipNetworks.sisyphos
  },
  amount: {
    type: "string",
    demandOption: true,
    describe: "The amount in Flipperino to fund"
  },
  "wallet-private-key": {
    type: "string",
    describe: "The private key of the wallet to use"
  },
  "state-chain-manager-contract-address": {
    type: "string",
    describe: "The contract address of the state chain manager when `chainflip-network` is `localnet`"
  },
  "flip-token-contract-address": {
    type: "string",
    describe: "The contract address for the FLIP token when `chainflip-network` is `localnet`"
  },
  "eth-network": {
    type: "string",
    describe: "The eth network URL to use when `chainflip-network` is `localnet`"
  }
};
async function cliFundStateChainAccount(args) {
  const privateKey = args.walletPrivateKey ?? await askForPrivateKey();
  const ethNetwork = getEthNetwork(args);
  const wallet = new Wallet(privateKey).connect(process.env.ALCHEMY_KEY ? new providers.AlchemyProvider(ethNetwork, process.env.ALCHEMY_KEY) : getDefaultProvider(ethNetwork));
  const opts = args.chainflipNetwork === "localnet" ? {
    stateChainGatewayContractAddress: args.stateChainManagerContractAddress,
    flipContractAddress: args.flipTokenContractAddress,
    signer: wallet,
    network: args.chainflipNetwork
  } : {
    network: args.chainflipNetwork,
    signer: wallet
  };
  const receipt = await fundStateChainAccount(args.srcAccountId, args.amount, opts);
  console.log(`Call executed. Transaction hash: ${receipt.transactionHash}`);
}
__name(cliFundStateChainAccount, "cliFundStateChainAccount");

// src/commands/cliRequestSwapDepositAddress.ts
var yargsOptions4 = {
  "src-asset": {
    choices: Object.values(Assets),
    describe: "The asset to swap from",
    demandOption: true
  },
  "dest-asset": {
    choices: Object.values(Assets),
    demandOption: true,
    describe: "The asset to swap to"
  },
  "dest-address": {
    type: "string",
    demandOption: true,
    describe: "The address to send the swapped assets to"
  },
  "broker-url": {
    type: "string",
    describe: "The broker URL",
    demandOption: true
  },
  "src-chain": {
    choices: Object.values(Chains),
    describe: "The chain to swap from",
    demandOption: true
  },
  "dest-chain": {
    choices: Object.values(Chains),
    describe: "The chain to swap to",
    demandOption: true
  }
};
async function cliRequestSwapDepositAddress(args) {
  const client = await BrokerClient.create({
    url: args.brokerUrl
  });
  const result = await client.requestSwapDepositAddress({
    srcAsset: args.srcAsset,
    destAsset: args.destAsset,
    destAddress: args.destAddress,
    srcChain: args.srcChain,
    destChain: args.destChain
  });
  console.log(`Deposit address: ${result.address}`);
  console.log(`Issued block: ${result.issuedBlock}`);
  console.log(`Expiry block: ${result.expiryBlock}`);
  console.log(`Channel ID: ${result.channelId}`);
  await client.close();
}
__name(cliRequestSwapDepositAddress, "cliRequestSwapDepositAddress");

// src/cli.ts
async function cli(args) {
  return yargs(args).scriptName("chainflip-cli").usage("$0 <cmd> [args]").command("swap", "", yargsOptions2, cliExecuteSwap).command("call", "", yargsOptions, cliExecuteCall).command("fund-state-chain-account", "", yargsOptions3, cliFundStateChainAccount).command("request-swap-deposit-address", "", yargsOptions4, cliRequestSwapDepositAddress).wrap(0).strict().help().parse();
}
__name(cli, "cli");

// src/main.ts
cli(process.argv.slice(2));
//# sourceMappingURL=out.js.map
//# sourceMappingURL=cli.mjs.map