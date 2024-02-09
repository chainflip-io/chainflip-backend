#! /usr/bin/env node
'use strict';

var yargs = require('yargs/yargs');
var ethers = require('ethers');
var utilCrypto = require('@polkadot/util-crypto');
var zod = require('zod');
var util = require('@polkadot/util');
var bech32Buffer = require('bech32-buffer');
var promises = require('readline/promises');
var axios = require('axios');
require('ioredis');

function _interopDefault (e) { return e && e.__esModule ? e : { default: e }; }

function _interopNamespace(e) {
  if (e && e.__esModule) return e;
  var n = Object.create(null);
  if (e) {
    Object.keys(e).forEach(function (k) {
      if (k !== 'default') {
        var d = Object.getOwnPropertyDescriptor(e, k);
        Object.defineProperty(n, k, d.get ? d : {
          enumerable: true,
          get: function () { return e[k]; }
        });
      }
    });
  }
  n.default = e;
  return Object.freeze(n);
}

var yargs__default = /*#__PURE__*/_interopDefault(yargs);
var ethers__namespace = /*#__PURE__*/_interopNamespace(ethers);
var axios__default = /*#__PURE__*/_interopDefault(axios);

var __defProp = Object.defineProperty;
var __name = (target, value) => __defProp(target, "name", { value, configurable: true });
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};

// ../shared/src/enums.ts
var arrayToMap = /* @__PURE__ */ __name((array) => Object.fromEntries(array.map((key) => [
  key,
  key
])), "arrayToMap");
var Chains = arrayToMap([
  "Bitcoin",
  "Ethereum",
  "Polkadot",
  "Arbitrum"
]);
var Assets = arrayToMap([
  "FLIP",
  "USDC",
  "DOT",
  "ETH",
  "BTC",
  "ARBETH",
  "ARBUSDC"
]);
var ChainflipNetworks = arrayToMap([
  "backspin",
  "sisyphos",
  "perseverance",
  "mainnet"
]);
var assetChains = {
  [Assets.ETH]: Chains.Ethereum,
  [Assets.FLIP]: Chains.Ethereum,
  [Assets.USDC]: Chains.Ethereum,
  [Assets.BTC]: Chains.Bitcoin,
  [Assets.DOT]: Chains.Polkadot,
  [Assets.ARBETH]: Chains.Arbitrum,
  [Assets.ARBUSDC]: Chains.Arbitrum
};
({
  [Assets.DOT]: 10,
  [Assets.ETH]: 18,
  [Assets.FLIP]: 18,
  [Assets.USDC]: 6,
  [Assets.BTC]: 8,
  [Assets.ARBETH]: 18,
  [Assets.ARBUSDC]: 6
});
var assetContractIds = {
  // 0 is reservered for particular cross chain messaging scenarios where we want to pass
  // through a message without making a swap.
  [Assets.ETH]: 1,
  [Assets.FLIP]: 2,
  [Assets.USDC]: 3,
  [Assets.DOT]: 4,
  [Assets.BTC]: 5,
  [Assets.ARBETH]: 6,
  [Assets.ARBUSDC]: 7
};
({
  [Chains.Ethereum]: [
    Assets.ETH,
    Assets.USDC,
    Assets.FLIP
  ],
  [Chains.Bitcoin]: [
    Assets.BTC
  ],
  [Chains.Polkadot]: [
    Assets.DOT
  ],
  [Chains.Arbitrum]: [
    Assets.ARBETH,
    Assets.ARBUSDC
  ]
});
({
  [Chains.Ethereum]: Assets.ETH,
  [Chains.Bitcoin]: Assets.BTC,
  [Chains.Polkadot]: Assets.DOT,
  [Chains.Arbitrum]: Assets.ARBETH
});
var chainContractIds = {
  [Chains.Ethereum]: 1,
  [Chains.Polkadot]: 2,
  [Chains.Bitcoin]: 3,
  [Chains.Arbitrum]: 4
};

// ../shared/src/guards.ts
var isString = /* @__PURE__ */ __name((value) => typeof value === "string", "isString");
var isNotNullish = /* @__PURE__ */ __name((value) => value !== null && value !== void 0, "isNotNullish");
function assert(condition, message) {
  if (condition)
    return;
  const error = new Error(message);
  if (error.stack) {
    error.stack = error.stack.replace(/\n.+/, "\n");
  }
  throw error;
}
__name(assert, "assert");
var isTokenSwap = /* @__PURE__ */ __name((params) => params.srcAsset !== Assets.ETH, "isTokenSwap");
var isTokenCall = /* @__PURE__ */ __name((params) => params.srcAsset !== Assets.ETH, "isTokenCall");
var isValidSegwitAddress = /* @__PURE__ */ __name((address) => {
  const hrp = /^(bc|tb|bcrt)1/.exec(address)?.[1];
  if (!hrp)
    return false;
  return bech32Buffer.BitcoinAddress.decode(address).prefix === hrp;
}, "isValidSegwitAddress");

// ../shared/src/validation/addressValidation.ts
var assertArraylikeEqual = /* @__PURE__ */ __name((a, b) => {
  assert(a.length === b.length, "arraylike lengths must be equal");
  for (let i = 0; i < a.length; i += 1) {
    assert(a[i] === b[i], "arraylike elements must be equal");
  }
}, "assertArraylikeEqual");
var validateP2PKHOrP2SHAddress = /* @__PURE__ */ __name((address, network) => {
  try {
    const decoded = utilCrypto.base58Decode(address);
    assert(decoded.length === 25, "decoded address must be 25 bytes long");
    if (network === "mainnet") {
      assert(decoded[0] === 0 || decoded[0] === 5, "decoded address must start with 0x00 or 0x05");
    } else {
      assert(decoded[0] === 111 || decoded[0] === 196, "decoded address must start with 0x6f or 0xc4");
    }
    const checksum = decoded.slice(-4);
    const doubleHash = ethers__namespace.getBytes(ethers__namespace.sha256(ethers__namespace.sha256(decoded.slice(0, 21))));
    assertArraylikeEqual(checksum, doubleHash.slice(0, 4));
    return true;
  } catch (error) {
    return false;
  }
}, "validateP2PKHOrP2SHAddress");
var validateSegwitAddress = /* @__PURE__ */ __name((address, network) => {
  try {
    assert(
      // On mainnet, the address must start with "bc1"
      network === "mainnet" && address.startsWith("bc1") || // on testnet it must start with "tb1"
      network === "testnet" && address.startsWith("tb1") || // on regtest it must start with "bcrt1"
      network === "regtest" && address.startsWith("bcrt1"),
      "address must start with bc1, tb1 or bcrt1"
    );
    return isValidSegwitAddress(address);
  } catch {
    return false;
  }
}, "validateSegwitAddress");
var validateBitcoinAddress = /* @__PURE__ */ __name((address, network) => validateP2PKHOrP2SHAddress(address, network) || validateSegwitAddress(address, network), "validateBitcoinAddress");
var validateBitcoinMainnetAddress = /* @__PURE__ */ __name((address) => validateBitcoinAddress(address, "mainnet"), "validateBitcoinMainnetAddress");
var validateBitcoinTestnetAddress = /* @__PURE__ */ __name((address) => validateBitcoinAddress(address, "testnet"), "validateBitcoinTestnetAddress");
var validateBitcoinRegtestAddress = /* @__PURE__ */ __name((address) => validateBitcoinAddress(address, "regtest"), "validateBitcoinRegtestAddress");

// ../shared/src/parsers.ts
var safeStringify = /* @__PURE__ */ __name((obj) => JSON.stringify(obj, (key, value) => typeof value === "bigint" ? value.toString() : value), "safeStringify");
var errorMap = /* @__PURE__ */ __name((_issue, context) => ({
  message: `received: ${safeStringify(context.data)}`
}), "errorMap");
var string = zod.z.string({
  errorMap
});
var number = zod.z.number({
  errorMap
});
var numericString = string.regex(/^[0-9]+$/);
var hexString = string.refine((v) => /^0x[0-9a-f]+$/i.test(v));
var hexStringWithMaxByteSize = /* @__PURE__ */ __name((maxByteSize) => hexString.refine((val) => val.length / 2 <= maxByteSize + 1, {
  message: `String must be less than or equal to ${maxByteSize} bytes`
}), "hexStringWithMaxByteSize");
var hexStringFromNumber = numericString.transform((arg) => `0x${BigInt(arg).toString(16)}`);
string.regex(/^[0-9a-f]+$/);
var btcAddress = /* @__PURE__ */ __name((network) => {
  if (network === "mainnet") {
    return string.regex(/^(1|3|bc1)/).refine(validateBitcoinMainnetAddress);
  }
  return zod.z.union([
    string.regex(/^(m|n|2|tb1)/).refine(validateBitcoinTestnetAddress),
    string.regex(/^bcrt1/).refine(validateBitcoinRegtestAddress)
  ]);
}, "btcAddress");
var DOT_PREFIX = 0;
var dotAddress = zod.z.union([
  string,
  hexString
]).transform((arg) => {
  try {
    if (arg.startsWith("0x")) {
      return utilCrypto.encodeAddress(util.hexToU8a(arg), DOT_PREFIX);
    }
    const hex = util.u8aToHex(utilCrypto.decodeAddress(arg));
    return utilCrypto.encodeAddress(hex, DOT_PREFIX);
  } catch {
    return null;
  }
}).refine(isString);
var ethereumAddress = hexString.refine((address) => ethers__namespace.isAddress(address));
numericString.transform((arg) => BigInt(arg));
var u128 = zod.z.union([
  number,
  numericString,
  hexString
]).transform((arg) => BigInt(arg));
var unsignedInteger = zod.z.union([
  u128,
  zod.z.number().transform((n) => BigInt(n))
]);
zod.z.object({
  __kind: zod.z.enum([
    "Usdc",
    "Flip",
    "Dot",
    "Eth",
    "Btc"
  ])
}).transform(({ __kind }) => __kind.toUpperCase());
var transformAsset = /* @__PURE__ */ __name((asset) => ({
  asset,
  chain: assetChains[asset]
}), "transformAsset");
var chainflipChain = zod.z.nativeEnum(Chains);
var chainflipAsset = zod.z.nativeEnum(Assets);
var chainflipAssetAndChain = zod.z.union([
  chainflipAsset.transform(transformAsset),
  zod.z.object({
    asset: zod.z.nativeEnum(Assets),
    chain: zod.z.nativeEnum(Chains)
  })
]).superRefine((obj, ctx) => {
  if (assetChains[obj.asset] !== obj.chain) {
    ctx.addIssue({
      code: zod.z.ZodIssueCode.custom,
      message: `asset ${obj.asset} does not belong to chain ${obj.chain}`,
      path: []
    });
  }
  return zod.z.NEVER;
});
var chainflipNetwork = zod.z.nativeEnum(ChainflipNetworks);
zod.z.union([
  zod.z.object({
    __kind: zod.z.literal("CcmPrincipal"),
    value: unsignedInteger
  }).transform(({ value: ccmId }) => ({
    type: "PRINCIPAL",
    ccmId
  })),
  zod.z.object({
    __kind: zod.z.literal("CcmGas"),
    value: unsignedInteger
  }).transform(({ value: ccmId }) => ({
    type: "GAS",
    ccmId
  })),
  zod.z.object({
    __kind: zod.z.literal("Swap")
  }).transform(() => ({
    type: "SWAP"
  }))
]);
zod.z.object({
  srcAsset: chainflipAssetAndChain,
  destAsset: chainflipAssetAndChain,
  amount: numericString,
  brokerCommissionBps: zod.z.string().regex(/^[0-9]*$/).transform((v) => Number(v)).optional()
});
var ccmMetadataSchema = zod.z.object({
  gasBudget: numericString,
  message: hexStringWithMaxByteSize(1024 * 10)
});
zod.z.object({
  srcAsset: chainflipAsset,
  destAsset: chainflipAsset,
  srcChain: chainflipChain,
  destChain: chainflipChain,
  destAddress: zod.z.string(),
  amount: numericString,
  ccmMetadata: ccmMetadataSchema.optional()
}).transform(({ amount, ...rest }) => ({
  ...rest,
  expectedDepositAmount: amount
}));

// ../shared/src/vault/schemas.ts
var bytesToHex = /* @__PURE__ */ __name((arr) => `0x${[
  ...arr
].map((v) => v.toString(16).padStart(2, "0")).join("")}`, "bytesToHex");
var utf8ToHex = /* @__PURE__ */ __name((str) => `0x${Buffer.from(str).toString("hex")}`, "utf8ToHex");
var eth = zod.z.object({
  amount: numericString,
  srcChain: zod.z.literal(Chains.Ethereum),
  srcAsset: zod.z.literal(Assets.ETH)
});
var ethToEthereum = eth.extend({
  destChain: zod.z.literal(Chains.Ethereum),
  destAddress: ethereumAddress
});
var ethToDot = eth.extend({
  destChain: zod.z.literal(Chains.Polkadot),
  destAddress: dotAddress.transform((addr) => bytesToHex(utilCrypto.decodeAddress(addr))),
  destAsset: zod.z.literal(Assets.DOT)
});
var ethToBtc = /* @__PURE__ */ __name((network) => eth.extend({
  destChain: zod.z.literal(Chains.Bitcoin),
  destAddress: btcAddress(network).transform(utf8ToHex),
  destAsset: zod.z.literal(Assets.BTC)
}), "ethToBtc");
var erc20Asset = zod.z.union([
  zod.z.literal(Assets.FLIP),
  zod.z.literal(Assets.USDC)
]);
var ethToERC20 = ethToEthereum.extend({
  destAsset: erc20Asset
});
var nativeSwapParamsSchema = /* @__PURE__ */ __name((network) => zod.z.union([
  ethToERC20,
  ethToDot,
  ethToBtc(network)
]), "nativeSwapParamsSchema");
var flipToEthereumAsset = ethToEthereum.extend({
  srcAsset: zod.z.literal(Assets.FLIP),
  destAsset: zod.z.union([
    zod.z.literal(Assets.USDC),
    zod.z.literal(Assets.ETH)
  ])
});
var usdcToEthereumAsset = ethToEthereum.extend({
  srcAsset: zod.z.literal(Assets.USDC),
  destAsset: zod.z.union([
    zod.z.literal(Assets.FLIP),
    zod.z.literal(Assets.ETH)
  ])
});
var erc20ToDot = ethToDot.extend({
  srcAsset: erc20Asset
});
var erc20ToBtc = /* @__PURE__ */ __name((network) => ethToBtc(network).extend({
  srcAsset: erc20Asset
}), "erc20ToBtc");
var tokenSwapParamsSchema = /* @__PURE__ */ __name((network) => zod.z.union([
  flipToEthereumAsset,
  usdcToEthereumAsset,
  erc20ToDot,
  erc20ToBtc(network)
]), "tokenSwapParamsSchema");
var ccmFlipToEthereumAssset = flipToEthereumAsset.extend({
  ccmMetadata: ccmMetadataSchema
});
var ccmUsdcToEthereumAsset = usdcToEthereumAsset.extend({
  ccmMetadata: ccmMetadataSchema
});
var tokenCallParamsSchema = zod.z.union([
  ccmFlipToEthereumAssset,
  ccmUsdcToEthereumAsset
]);
var nativeCallParamsSchema = ethToERC20.extend({
  ccmMetadata: ccmMetadataSchema
});
var executeSwapParamsSchema = /* @__PURE__ */ __name((network) => zod.z.union([
  // call schemas needs to precede swap schemas
  nativeCallParamsSchema,
  tokenCallParamsSchema,
  nativeSwapParamsSchema(network),
  tokenSwapParamsSchema(network)
]), "executeSwapParamsSchema");
var _abi = [
  {
    inputs: [
      {
        internalType: "address",
        name: "owner",
        type: "address"
      },
      {
        internalType: "address",
        name: "spender",
        type: "address"
      }
    ],
    name: "allowance",
    outputs: [
      {
        internalType: "uint256",
        name: "",
        type: "uint256"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "spender",
        type: "address"
      },
      {
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "approve",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool"
      }
    ],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "account",
        type: "address"
      }
    ],
    name: "balanceOf",
    outputs: [
      {
        internalType: "uint256",
        name: "",
        type: "uint256"
      }
    ],
    stateMutability: "view",
    type: "function"
  }
];
var ERC20__factory = class {
  static {
    __name(this, "ERC20__factory");
  }
  static abi = _abi;
  static createInterface() {
    return new ethers.Interface(_abi);
  }
  static connect(address, runner) {
    return new ethers.Contract(address, _abi, runner);
  }
};
var _abi2 = [
  {
    inputs: [
      {
        internalType: "contract IKeyManager",
        name: "keyManager",
        type: "address"
      },
      {
        internalType: "uint256",
        name: "minFunding",
        type: "uint256"
      }
    ],
    stateMutability: "nonpayable",
    type: "constructor"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "bool",
        name: "communityGuardDisabled",
        type: "bool"
      }
    ],
    name: "CommunityGuardDisabled",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "flip",
        type: "address"
      }
    ],
    name: "FLIPSet",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "uint256",
        name: "oldSupply",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "newSupply",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "stateChainBlockNumber",
        type: "uint256"
      }
    ],
    name: "FlipSupplyUpdated",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "address",
        name: "funder",
        type: "address"
      }
    ],
    name: "Funded",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "to",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "GovernanceWithdrawal",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "uint256",
        name: "oldMinFunding",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "newMinFunding",
        type: "uint256"
      }
    ],
    name: "MinFundingChanged",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "RedemptionExecuted",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "RedemptionExpired",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "redeemAddress",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint48",
        name: "startTime",
        type: "uint48"
      },
      {
        indexed: false,
        internalType: "uint48",
        name: "expiryTime",
        type: "uint48"
      }
    ],
    name: "RedemptionRegistered",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "bool",
        name: "suspended",
        type: "bool"
      }
    ],
    name: "Suspended",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "keyManager",
        type: "address"
      }
    ],
    name: "UpdatedKeyManager",
    type: "event"
  },
  {
    inputs: [],
    name: "REDEMPTION_DELAY",
    outputs: [
      {
        internalType: "uint48",
        name: "",
        type: "uint48"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "disableCommunityGuard",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "enableCommunityGuard",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      }
    ],
    name: "executeRedemption",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      },
      {
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "fundStateChainAccount",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "getCommunityGuardDisabled",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getCommunityKey",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getFLIP",
    outputs: [
      {
        internalType: "contract IFLIP",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getGovernor",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getKeyManager",
    outputs: [
      {
        internalType: "contract IKeyManager",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getLastSupplyUpdateBlockNumber",
    outputs: [
      {
        internalType: "uint256",
        name: "",
        type: "uint256"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getMinimumFunding",
    outputs: [
      {
        internalType: "uint256",
        name: "",
        type: "uint256"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      }
    ],
    name: "getPendingRedemption",
    outputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "amount",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "redeemAddress",
            type: "address"
          },
          {
            internalType: "uint48",
            name: "startTime",
            type: "uint48"
          },
          {
            internalType: "uint48",
            name: "expiryTime",
            type: "uint48"
          }
        ],
        internalType: "struct IStateChainGateway.Redemption",
        name: "",
        type: "tuple"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getSuspendedState",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "govUpdateFlipIssuer",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "govWithdraw",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        internalType: "bytes32",
        name: "nodeID",
        type: "bytes32"
      },
      {
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        internalType: "address",
        name: "redeemAddress",
        type: "address"
      },
      {
        internalType: "uint48",
        name: "expiryTime",
        type: "uint48"
      }
    ],
    name: "registerRedemption",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "resume",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "contract IFLIP",
        name: "flip",
        type: "address"
      }
    ],
    name: "setFlip",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "uint256",
        name: "newMinFunding",
        type: "uint256"
      }
    ],
    name: "setMinFunding",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "suspend",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        internalType: "address",
        name: "newIssuer",
        type: "address"
      },
      {
        internalType: "bool",
        name: "omitChecks",
        type: "bool"
      }
    ],
    name: "updateFlipIssuer",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        internalType: "uint256",
        name: "newTotalSupply",
        type: "uint256"
      },
      {
        internalType: "uint256",
        name: "stateChainBlockNumber",
        type: "uint256"
      }
    ],
    name: "updateFlipSupply",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        internalType: "contract IKeyManager",
        name: "keyManager",
        type: "address"
      },
      {
        internalType: "bool",
        name: "omitChecks",
        type: "bool"
      }
    ],
    name: "updateKeyManager",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  }
];
var StateChainGateway__factory = class {
  static {
    __name(this, "StateChainGateway__factory");
  }
  static abi = _abi2;
  static createInterface() {
    return new ethers.Interface(_abi2);
  }
  static connect(address, runner) {
    return new ethers.Contract(address, _abi2, runner);
  }
};
var _abi3 = [
  {
    inputs: [
      {
        internalType: "contract IKeyManager",
        name: "keyManager",
        type: "address"
      }
    ],
    stateMutability: "nonpayable",
    type: "constructor"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "bytes32",
        name: "swapID",
        type: "bytes32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "AddGasNative",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "bytes32",
        name: "swapID",
        type: "bytes32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "address",
        name: "token",
        type: "address"
      }
    ],
    name: "AddGasToken",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "bool",
        name: "communityGuardDisabled",
        type: "bool"
      }
    ],
    name: "CommunityGuardDisabled",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "address payable",
        name: "multicallAddress",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "token",
        type: "address"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "reason",
        type: "bytes"
      }
    ],
    name: "ExecuteActionsFailed",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "bool",
        name: "suspended",
        type: "bool"
      }
    ],
    name: "Suspended",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        indexed: false,
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "sender",
        type: "address"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "SwapNative",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        indexed: false,
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "address",
        name: "srcToken",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "sender",
        type: "address"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "SwapToken",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "address payable",
        name: "recipient",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      }
    ],
    name: "TransferNativeFailed",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: true,
        internalType: "address payable",
        name: "recipient",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "token",
        type: "address"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "reason",
        type: "bytes"
      }
    ],
    name: "TransferTokenFailed",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "keyManager",
        type: "address"
      }
    ],
    name: "UpdatedKeyManager",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        indexed: false,
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "sender",
        type: "address"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "message",
        type: "bytes"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "gasAmount",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "XCallNative",
    type: "event"
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        indexed: false,
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        indexed: false,
        internalType: "address",
        name: "srcToken",
        type: "address"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        indexed: true,
        internalType: "address",
        name: "sender",
        type: "address"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "message",
        type: "bytes"
      },
      {
        indexed: false,
        internalType: "uint256",
        name: "gasAmount",
        type: "uint256"
      },
      {
        indexed: false,
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "XCallToken",
    type: "event"
  },
  {
    inputs: [
      {
        internalType: "bytes32",
        name: "swapID",
        type: "bytes32"
      }
    ],
    name: "addGasNative",
    outputs: [],
    stateMutability: "payable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "bytes32",
        name: "swapID",
        type: "bytes32"
      },
      {
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        internalType: "contract IERC20",
        name: "token",
        type: "address"
      }
    ],
    name: "addGasToken",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "bytes32",
            name: "swapID",
            type: "bytes32"
          },
          {
            internalType: "address",
            name: "token",
            type: "address"
          }
        ],
        internalType: "struct IShared.DeployFetchParams[]",
        name: "deployFetchParamsArray",
        type: "tuple[]"
      },
      {
        components: [
          {
            internalType: "address payable",
            name: "fetchContract",
            type: "address"
          },
          {
            internalType: "address",
            name: "token",
            type: "address"
          }
        ],
        internalType: "struct IShared.FetchParams[]",
        name: "fetchParamsArray",
        type: "tuple[]"
      },
      {
        components: [
          {
            internalType: "address",
            name: "token",
            type: "address"
          },
          {
            internalType: "address payable",
            name: "recipient",
            type: "address"
          },
          {
            internalType: "uint256",
            name: "amount",
            type: "uint256"
          }
        ],
        internalType: "struct IShared.TransferParams[]",
        name: "transferParamsArray",
        type: "tuple[]"
      }
    ],
    name: "allBatch",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "bytes32",
            name: "swapID",
            type: "bytes32"
          },
          {
            internalType: "address",
            name: "token",
            type: "address"
          }
        ],
        internalType: "struct IShared.DeployFetchParams[]",
        name: "deployFetchParamsArray",
        type: "tuple[]"
      }
    ],
    name: "deployAndFetchBatch",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "disableCommunityGuard",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "enableCommunityGuard",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "address",
            name: "token",
            type: "address"
          },
          {
            internalType: "address payable",
            name: "recipient",
            type: "address"
          },
          {
            internalType: "uint256",
            name: "amount",
            type: "uint256"
          }
        ],
        internalType: "struct IShared.TransferParams",
        name: "transferParams",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "enum IMulticall.CallType",
            name: "callType",
            type: "uint8"
          },
          {
            internalType: "address",
            name: "target",
            type: "address"
          },
          {
            internalType: "uint256",
            name: "value",
            type: "uint256"
          },
          {
            internalType: "bytes",
            name: "callData",
            type: "bytes"
          },
          {
            internalType: "bytes",
            name: "payload",
            type: "bytes"
          }
        ],
        internalType: "struct IMulticall.Call[]",
        name: "calls",
        type: "tuple[]"
      },
      {
        internalType: "uint256",
        name: "gasMulticall",
        type: "uint256"
      }
    ],
    name: "executeActions",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        internalType: "address",
        name: "recipient",
        type: "address"
      },
      {
        internalType: "uint32",
        name: "srcChain",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "srcAddress",
        type: "bytes"
      },
      {
        internalType: "bytes",
        name: "message",
        type: "bytes"
      }
    ],
    name: "executexCall",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "address",
            name: "token",
            type: "address"
          },
          {
            internalType: "address payable",
            name: "recipient",
            type: "address"
          },
          {
            internalType: "uint256",
            name: "amount",
            type: "uint256"
          }
        ],
        internalType: "struct IShared.TransferParams",
        name: "transferParams",
        type: "tuple"
      },
      {
        internalType: "uint32",
        name: "srcChain",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "srcAddress",
        type: "bytes"
      },
      {
        internalType: "bytes",
        name: "message",
        type: "bytes"
      }
    ],
    name: "executexSwapAndCall",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "address payable",
            name: "fetchContract",
            type: "address"
          },
          {
            internalType: "address",
            name: "token",
            type: "address"
          }
        ],
        internalType: "struct IShared.FetchParams[]",
        name: "fetchParamsArray",
        type: "tuple[]"
      }
    ],
    name: "fetchBatch",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "getCommunityGuardDisabled",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getCommunityKey",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getGovernor",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getKeyManager",
    outputs: [
      {
        internalType: "contract IKeyManager",
        name: "",
        type: "address"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [],
    name: "getSuspendedState",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool"
      }
    ],
    stateMutability: "view",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "address[]",
        name: "tokens",
        type: "address[]"
      }
    ],
    name: "govWithdraw",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "resume",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [],
    name: "suspend",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "address",
            name: "token",
            type: "address"
          },
          {
            internalType: "address payable",
            name: "recipient",
            type: "address"
          },
          {
            internalType: "uint256",
            name: "amount",
            type: "uint256"
          }
        ],
        internalType: "struct IShared.TransferParams",
        name: "transferParams",
        type: "tuple"
      }
    ],
    name: "transfer",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        components: [
          {
            internalType: "address",
            name: "token",
            type: "address"
          },
          {
            internalType: "address payable",
            name: "recipient",
            type: "address"
          },
          {
            internalType: "uint256",
            name: "amount",
            type: "uint256"
          }
        ],
        internalType: "struct IShared.TransferParams[]",
        name: "transferParamsArray",
        type: "tuple[]"
      }
    ],
    name: "transferBatch",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        components: [
          {
            internalType: "uint256",
            name: "sig",
            type: "uint256"
          },
          {
            internalType: "uint256",
            name: "nonce",
            type: "uint256"
          },
          {
            internalType: "address",
            name: "kTimesGAddress",
            type: "address"
          }
        ],
        internalType: "struct IShared.SigData",
        name: "sigData",
        type: "tuple"
      },
      {
        internalType: "contract IKeyManager",
        name: "keyManager",
        type: "address"
      },
      {
        internalType: "bool",
        name: "omitChecks",
        type: "bool"
      }
    ],
    name: "updateKeyManager",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "message",
        type: "bytes"
      },
      {
        internalType: "uint256",
        name: "gasAmount",
        type: "uint256"
      },
      {
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "xCallNative",
    outputs: [],
    stateMutability: "payable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "message",
        type: "bytes"
      },
      {
        internalType: "uint256",
        name: "gasAmount",
        type: "uint256"
      },
      {
        internalType: "contract IERC20",
        name: "srcToken",
        type: "address"
      },
      {
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "xCallToken",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "xSwapNative",
    outputs: [],
    stateMutability: "payable",
    type: "function"
  },
  {
    inputs: [
      {
        internalType: "uint32",
        name: "dstChain",
        type: "uint32"
      },
      {
        internalType: "bytes",
        name: "dstAddress",
        type: "bytes"
      },
      {
        internalType: "uint32",
        name: "dstToken",
        type: "uint32"
      },
      {
        internalType: "contract IERC20",
        name: "srcToken",
        type: "address"
      },
      {
        internalType: "uint256",
        name: "amount",
        type: "uint256"
      },
      {
        internalType: "bytes",
        name: "cfParameters",
        type: "bytes"
      }
    ],
    name: "xSwapToken",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function"
  },
  {
    stateMutability: "payable",
    type: "receive"
  }
];
var Vault__factory = class {
  static {
    __name(this, "Vault__factory");
  }
  static abi = _abi3;
  static createInterface() {
    return new ethers.Interface(_abi3);
  }
  static connect(address, runner) {
    return new ethers.Contract(address, _abi3, runner);
  }
};

// ../shared/src/consts.ts
({
  [ChainflipNetworks.backspin]: 1e3,
  [ChainflipNetworks.sisyphos]: 1e3,
  [ChainflipNetworks.perseverance]: 1e3,
  [ChainflipNetworks.mainnet]: 1e3
});
var GOERLI_USDC_CONTRACT_ADDRESS = "0x07865c6E87B9F70255377e024ace6630C1Eaa37F";
var ADDRESSES = {
  [ChainflipNetworks.backspin]: {
    FLIP_CONTRACT_ADDRESS: "0x10C6E9530F1C1AF873a391030a1D9E8ed0630D26",
    USDC_CONTRACT_ADDRESS: "0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0",
    VAULT_CONTRACT_ADDRESS: "0xB7A5bd0345EF1Cc5E66bf61BdeC17D2461fBd968",
    STATE_CHAIN_GATEWAY_ADDRESS: "0xeEBe00Ac0756308ac4AaBfD76c05c4F3088B8883"
  },
  [ChainflipNetworks.sisyphos]: {
    FLIP_CONTRACT_ADDRESS: "0x2BbB561C6eaB74f358cA9e8a961E3A20CAE3D100",
    USDC_CONTRACT_ADDRESS: GOERLI_USDC_CONTRACT_ADDRESS,
    VAULT_CONTRACT_ADDRESS: "0xC17CCec5015081EB2DF26d20A9e02c5484C1d641",
    STATE_CHAIN_GATEWAY_ADDRESS: "0xE8bE4B7F8a38C1913387c9C20B94402bc3Db9F70"
  },
  [ChainflipNetworks.perseverance]: {
    FLIP_CONTRACT_ADDRESS: "0x0485D65da68b2A6b48C3fA28D7CCAce196798B94",
    USDC_CONTRACT_ADDRESS: GOERLI_USDC_CONTRACT_ADDRESS,
    VAULT_CONTRACT_ADDRESS: "0x40caFF3f3B6706Da904a7895e0fC7F7922437e9B",
    STATE_CHAIN_GATEWAY_ADDRESS: "0x38AA40B7b5a70d738baBf6699a45DacdDBBEB3fc"
  },
  [ChainflipNetworks.mainnet]: {
    FLIP_CONTRACT_ADDRESS: "0x826180541412D574cf1336d22c0C0a287822678A",
    USDC_CONTRACT_ADDRESS: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    VAULT_CONTRACT_ADDRESS: "0xF5e10380213880111522dd0efD3dbb45b9f62Bcc",
    STATE_CHAIN_GATEWAY_ADDRESS: "0x6995Ab7c4D7F4B03f467Cf4c8E920427d9621DBd"
  }
};

// ../shared/src/contracts.ts
var extractOverrides = /* @__PURE__ */ __name((transactionOverrides) => {
  const { wait, ...ethersOverrides } = transactionOverrides;
  return ethersOverrides;
}, "extractOverrides");
function getTokenContractAddress(asset, network, assert2 = true) {
  if (asset === Assets.FLIP)
    return ADDRESSES[network].FLIP_CONTRACT_ADDRESS;
  if (asset === Assets.USDC)
    return ADDRESSES[network].USDC_CONTRACT_ADDRESS;
  if (assert2) {
    throw new Error("Only FLIP and USDC are supported for now");
  }
  return void 0;
}
__name(getTokenContractAddress, "getTokenContractAddress");
var getStateChainGatewayContractAddress = /* @__PURE__ */ __name((network) => ADDRESSES[network].STATE_CHAIN_GATEWAY_ADDRESS, "getStateChainGatewayContractAddress");
var checkAllowance = /* @__PURE__ */ __name(async (amount, spenderAddress, erc20Address, signer) => {
  const erc20 = ERC20__factory.connect(erc20Address, signer);
  const signerAddress = await signer.getAddress();
  const allowance = await erc20.allowance(signerAddress, spenderAddress);
  return {
    allowance,
    isAllowable: allowance >= amount,
    erc20
  };
}, "checkAllowance");
var getVaultManagerContractAddress = /* @__PURE__ */ __name((network) => ADDRESSES[network].VAULT_CONTRACT_ADDRESS, "getVaultManagerContractAddress");

// ../shared/src/vault/executeSwap.ts
var swapNative = /* @__PURE__ */ __name(async ({ destChain, destAsset, destAddress, amount }, networkOpts, txOpts) => {
  const vaultContractAddress = networkOpts.network === "localnet" ? networkOpts.vaultContractAddress : getVaultManagerContractAddress(networkOpts.network);
  const vault = Vault__factory.connect(vaultContractAddress, networkOpts.signer);
  const transaction = await vault.xSwapNative(chainContractIds[destChain], destAddress, assetContractIds[destAsset], "0x", {
    value: amount,
    ...extractOverrides(txOpts)
  });
  return transaction.wait(txOpts.wait);
}, "swapNative");
var swapToken = /* @__PURE__ */ __name(async (params, networkOpts, txOpts) => {
  const vaultContractAddress = networkOpts.network === "localnet" ? networkOpts.vaultContractAddress : getVaultManagerContractAddress(networkOpts.network);
  const erc20Address = networkOpts.network === "localnet" ? networkOpts.srcTokenContractAddress : getTokenContractAddress(params.srcAsset, networkOpts.network);
  assert(erc20Address !== void 0, "Missing ERC20 contract address");
  const { isAllowable } = await checkAllowance(BigInt(params.amount), vaultContractAddress, erc20Address, networkOpts.signer);
  assert(isAllowable, "Swap amount exceeds allowance");
  const vault = Vault__factory.connect(vaultContractAddress, networkOpts.signer);
  const transaction = await vault.xSwapToken(chainContractIds[params.destChain], params.destAddress, assetContractIds[params.destAsset], erc20Address, params.amount, "0x", extractOverrides(txOpts));
  return transaction.wait(txOpts.wait);
}, "swapToken");
var callNative = /* @__PURE__ */ __name(async (params, networkOpts, txOpts) => {
  const vaultContractAddress = networkOpts.network === "localnet" ? networkOpts.vaultContractAddress : getVaultManagerContractAddress(networkOpts.network);
  const vault = Vault__factory.connect(vaultContractAddress, networkOpts.signer);
  const transaction = await vault.xCallNative(chainContractIds[params.destChain], params.destAddress, assetContractIds[params.destAsset], params.ccmMetadata.message, params.ccmMetadata.gasBudget, "0x", {
    value: params.amount,
    ...extractOverrides(txOpts)
  });
  return transaction.wait(txOpts.wait);
}, "callNative");
var callToken = /* @__PURE__ */ __name(async (params, networkOpts, txOpts) => {
  const vaultContractAddress = networkOpts.network === "localnet" ? networkOpts.vaultContractAddress : getVaultManagerContractAddress(networkOpts.network);
  const erc20Address = networkOpts.network === "localnet" ? networkOpts.srcTokenContractAddress : getTokenContractAddress(params.srcAsset, networkOpts.network);
  assert(erc20Address !== void 0, "Missing ERC20 contract address");
  const { isAllowable } = await checkAllowance(BigInt(params.amount), vaultContractAddress, erc20Address, networkOpts.signer);
  assert(isAllowable, "Swap amount exceeds allowance");
  const vault = Vault__factory.connect(vaultContractAddress, networkOpts.signer);
  const transaction = await vault.xCallToken(chainContractIds[params.destChain], params.destAddress, assetContractIds[params.destAsset], params.ccmMetadata.message, params.ccmMetadata.gasBudget, erc20Address, params.amount, "0x", extractOverrides(txOpts));
  return transaction.wait(txOpts.wait);
}, "callToken");
var executeSwap = /* @__PURE__ */ __name(async (params, networkOpts, txOpts) => {
  const network = networkOpts.network === "localnet" ? "backspin" : networkOpts.network;
  const parsedParams = executeSwapParamsSchema(network).parse(params);
  if ("ccmMetadata" in parsedParams) {
    return isTokenCall(parsedParams) ? callToken(parsedParams, networkOpts, txOpts) : callNative(parsedParams, networkOpts, txOpts);
  }
  return isTokenSwap(parsedParams) ? swapToken(parsedParams, networkOpts, txOpts) : swapNative(parsedParams, networkOpts, txOpts);
}, "executeSwap");
var executeSwap_default = executeSwap;
var askForPrivateKey = /* @__PURE__ */ __name(async () => {
  const rl = promises.createInterface({
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

// src/commands/cliExecuteSwap.ts
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
    describe: "The message that is sent along with the swapped assets"
  },
  "gas-budget": {
    type: "string",
    describe: "The amount of gas that is sent with the message"
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
  const wallet = new ethers.Wallet(privateKey).connect(process.env.ALCHEMY_KEY ? new ethers.AlchemyProvider(ethNetwork, process.env.ALCHEMY_KEY) : ethers.getDefaultProvider(ethNetwork));
  const networkOpts = args.chainflipNetwork === "localnet" ? {
    vaultContractAddress: args.vaultContractAddress,
    srcTokenContractAddress: args.srcTokenContractAddress,
    signer: wallet,
    network: args.chainflipNetwork
  } : {
    network: args.chainflipNetwork,
    signer: wallet
  };
  let ccmMetadata;
  if (args.gasBudget || args.message) {
    assert(args.gasBudget, "missing gas budget");
    assert(args.message, "missing message");
    ccmMetadata = {
      message: args.message,
      gasBudget: args.gasBudget
    };
  }
  const receipt = await executeSwap_default({
    srcChain: assetChains[args.srcAsset],
    srcAsset: args.srcAsset,
    destChain: assetChains[args.destAsset],
    destAsset: args.destAsset,
    amount: args.amount,
    destAddress: args.destAddress,
    ccmMetadata
  }, networkOpts, {});
  console.log(`Swap executed. Transaction hash: ${receipt.hash}`);
}
__name(cliExecuteSwap, "cliExecuteSwap");

// ../shared/src/stateChainGateway/utils.ts
var getStateChainGateway = /* @__PURE__ */ __name((networkOpts) => {
  const stateChainGatewayContractAddress = networkOpts.network === "localnet" ? networkOpts.stateChainGatewayContractAddress : getStateChainGatewayContractAddress(networkOpts.network);
  return StateChainGateway__factory.connect(stateChainGatewayContractAddress, networkOpts.signer);
}, "getStateChainGateway");

// ../shared/src/stateChainGateway/index.ts
var fundStateChainAccount = /* @__PURE__ */ __name(async (accountId, amount, networkOpts, txOpts) => {
  const flipContractAddress = networkOpts.network === "localnet" ? networkOpts.flipContractAddress : getTokenContractAddress(Assets.FLIP, networkOpts.network);
  const stateChainGateway = getStateChainGateway(networkOpts);
  const { isAllowable } = await checkAllowance(amount, await stateChainGateway.getAddress(), flipContractAddress, networkOpts.signer);
  assert(isAllowable, "Insufficient allowance");
  const transaction = await stateChainGateway.fundStateChainAccount(accountId, amount, extractOverrides(txOpts));
  return transaction.wait(txOpts.wait);
}, "fundStateChainAccount");

// ../shared/src/broker.ts
var broker_exports = {};
__export(broker_exports, {
  requestSwapDepositAddress: () => requestSwapDepositAddress
});

// ../shared/src/strings.ts
var camelToSnakeCase = /* @__PURE__ */ __name((str) => str.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`), "camelToSnakeCase");

// ../shared/src/broker.ts
var transformObjToSnakeCase = /* @__PURE__ */ __name((obj) => {
  if (!obj)
    return void 0;
  const newObj = {};
  for (const key in obj) {
    if (Object.prototype.hasOwnProperty.call(obj, key)) {
      newObj[camelToSnakeCase(key)] = obj[key];
    }
  }
  return newObj;
}, "transformObjToSnakeCase");
var submitAddress = /* @__PURE__ */ __name((asset, address) => {
  if (asset === Assets.DOT) {
    return address.startsWith("0x") ? zod.z.string().length(66).parse(address) : util.u8aToHex(utilCrypto.decodeAddress(address));
  }
  return address;
}, "submitAddress");
var rpcResult = zod.z.union([
  zod.z.object({
    error: zod.z.object({
      code: zod.z.number().optional(),
      message: zod.z.string().optional(),
      data: zod.z.unknown().optional()
    })
  }),
  zod.z.object({
    result: zod.z.unknown()
  })
]);
var requestValidators = /* @__PURE__ */ __name((network) => ({
  requestSwapDepositAddress: zod.z.tuple([
    chainflipAssetAndChain,
    chainflipAssetAndChain,
    zod.z.union([
      numericString,
      hexString,
      btcAddress(network)
    ]),
    zod.z.number(),
    ccmMetadataSchema.merge(zod.z.object({
      gasBudget: hexStringFromNumber,
      cfParameters: zod.z.union([
        hexString,
        zod.z.string()
      ]).optional()
    })).optional()
  ]).transform(([a, b, c, d, e]) => [
    a,
    b,
    c,
    d,
    transformObjToSnakeCase(e)
  ].filter(isNotNullish))
}), "requestValidators");
var responseValidators = /* @__PURE__ */ __name((network) => ({
  requestSwapDepositAddress: zod.z.object({
    address: zod.z.union([
      dotAddress,
      hexString,
      btcAddress(network)
    ]),
    issued_block: zod.z.number(),
    channel_id: zod.z.number(),
    expiry_block: zod.z.number().int().safe().positive().optional(),
    source_chain_expiry_block: unsignedInteger.optional()
  }).transform(({ address, issued_block, channel_id, source_chain_expiry_block }) => ({
    address,
    issuedBlock: issued_block,
    channelId: BigInt(channel_id),
    sourceChainExpiryBlock: source_chain_expiry_block
  }))
}), "responseValidators");
var makeRpcRequest = /* @__PURE__ */ __name(async (network, url, method, ...params) => {
  const res = await axios__default.default.post(url.toString(), {
    jsonrpc: "2.0",
    id: 1,
    method: `broker_${method}`,
    params: requestValidators(network)[method].parse(params)
  });
  const result = rpcResult.parse(res.data);
  if ("error" in result) {
    throw new Error(`Broker responded with error code ${result.error.code}: ${result.error.message}`);
  }
  return responseValidators(network)[method].parse(result.result);
}, "makeRpcRequest");
async function requestSwapDepositAddress(swapRequest, opts, chainflipNetwork2) {
  const { srcAsset, srcChain, destAsset, destChain, destAddress } = swapRequest;
  return makeRpcRequest(chainflipNetwork2, opts.url, "requestSwapDepositAddress", {
    asset: srcAsset,
    chain: srcChain
  }, {
    asset: destAsset,
    chain: destChain
  }, submitAddress(destAsset, destAddress), opts.commissionBps, swapRequest.ccmMetadata && {
    ...swapRequest.ccmMetadata,
    cfParameters: void 0
  });
}
__name(requestSwapDepositAddress, "requestSwapDepositAddress");

// ../shared/src/arrays.ts
var sorter = /* @__PURE__ */ __name((key, dir = "asc") => (a, b) => {
  let result = 0;
  if (a[key] < b[key]) {
    result = -1;
  } else if (a[key] > b[key]) {
    result = 1;
  }
  return dir === "asc" ? result : -result;
}, "sorter");

// ../shared/src/node-apis/redis.ts
var jsonString = string.transform((value) => JSON.parse(value));
jsonString.pipe(zod.z.object({
  amount: u128,
  asset: string,
  deposit_chain_block_height: number
}));
sorter("deposit_chain_block_height");
({
  Ethereum: zod.z.object({
    tx_out_id: zod.z.object({
      signature: zod.z.object({
        k_times_g_address: zod.z.array(number),
        s: zod.z.array(number)
      })
    })
  }),
  Polkadot: zod.z.object({
    tx_out_id: zod.z.object({
      signature: string
    })
  }),
  Bitcoin: zod.z.object({
    tx_out_id: zod.z.object({
      hash: string
    })
  }),
  Arbitrum: zod.z.object({
    tx_out_id: zod.z.object({
      signature: string
    })
  })
});
jsonString.pipe(zod.z.object({
  confirmations: number,
  value: u128,
  tx_hash: string.transform((value) => `0x${value}`)
}));

// src/commands/cliFundStateChainAccount.ts
var yargsOptions2 = {
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
  const wallet = new ethers.Wallet(privateKey).connect(process.env.ALCHEMY_KEY ? new ethers.AlchemyProvider(ethNetwork, process.env.ALCHEMY_KEY) : ethers.getDefaultProvider(ethNetwork));
  const networkOpts = args.chainflipNetwork === "localnet" ? {
    stateChainGatewayContractAddress: args.stateChainManagerContractAddress,
    flipContractAddress: args.flipTokenContractAddress,
    signer: wallet,
    network: args.chainflipNetwork
  } : {
    network: args.chainflipNetwork,
    signer: wallet
  };
  const receipt = await fundStateChainAccount(args.srcAccountId, BigInt(args.amount), networkOpts, {});
  console.log(`Call executed. Transaction hash: ${receipt.hash}`);
}
__name(cliFundStateChainAccount, "cliFundStateChainAccount");

// src/commands/cliRequestSwapDepositAddress.ts
var yargsOptions3 = {
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
  },
  message: {
    type: "string",
    describe: "The CCM message that is sent along with the swapped assets"
  },
  "gas-budget": {
    type: "string",
    describe: "The amount of gas that is sent with the CCM message"
  },
  network: {
    type: "string",
    demandOption: true,
    choices: [
      "mainnet",
      "perseverance",
      "backspin",
      "sisyphos"
    ]
  }
};
async function cliRequestSwapDepositAddress(args) {
  let ccmMetadata;
  if (args.gasBudget || args.message) {
    assert(args.gasBudget, "missing gas budget");
    assert(args.message, "missing message");
    ccmMetadata = {
      gasBudget: args.gasBudget,
      message: args.message
    };
  }
  const result = await broker_exports.requestSwapDepositAddress({
    srcAsset: args.srcAsset,
    destAsset: args.destAsset,
    destAddress: args.destAddress,
    srcChain: args.srcChain,
    destChain: args.destChain,
    ccmMetadata
  }, {
    url: args.brokerUrl,
    commissionBps: 0
  }, args.network);
  console.log(`Deposit address: ${result.address}`);
  console.log(`Issued block: ${result.issuedBlock}`);
  console.log(`Channel ID: ${result.channelId}`);
}
__name(cliRequestSwapDepositAddress, "cliRequestSwapDepositAddress");

// src/cli.ts
async function cli(args) {
  return yargs__default.default(args).scriptName("chainflip-cli").usage("$0 <cmd> [args]").command("swap", "", yargsOptions, cliExecuteSwap).command("fund-state-chain-account", "", yargsOptions2, cliFundStateChainAccount).command("request-swap-deposit-address", "", yargsOptions3, cliRequestSwapDepositAddress).wrap(0).strict().help().parse();
}
__name(cli, "cli");

// src/main.ts
cli(process.argv.slice(2));
//# sourceMappingURL=out.js.map
//# sourceMappingURL=cli.js.map