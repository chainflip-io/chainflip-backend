/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/cf_tester.json`.
 */
export type CfTester = {
  "address": "8pBPaVfTAcjLeNfC187Fkvi9b1XEFhRNJ95BQXXVksmH",
  "metadata": {
    "name": "cfTester",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Created with Anchor"
  },
  "instructions": [
    {
      "name": "cfReceiveNative",
      "discriminator": [
        228,
        51,
        109,
        5,
        176,
        83,
        201,
        81
      ],
      "accounts": [
        {
          "name": "receiverNative",
          "writable": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "instructionSysvar",
          "address": "Sysvar1nstructions1111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "sourceChain",
          "type": "u32"
        },
        {
          "name": "sourceAddress",
          "type": "bytes"
        },
        {
          "name": "message",
          "type": "bytes"
        },
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "cfReceiveToken",
      "discriminator": [
        66,
        95,
        143,
        16,
        13,
        3,
        3,
        83
      ],
      "accounts": [
        {
          "name": "receiverTokenAccount",
          "writable": true
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "mint"
        },
        {
          "name": "instructionSysvar",
          "address": "Sysvar1nstructions1111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "sourceChain",
          "type": "u32"
        },
        {
          "name": "sourceAddress",
          "type": "bytes"
        },
        {
          "name": "message",
          "type": "bytes"
        },
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "contractSwapNative",
      "discriminator": [
        254,
        187,
        162,
        188,
        29,
        232,
        174,
        193
      ],
      "accounts": [
        {
          "name": "vault"
        },
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "writable": true
        },
        {
          "name": "pda",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "arg",
                "path": "seed"
              }
            ]
          }
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "eventAuthority"
        }
      ],
      "args": [
        {
          "name": "seed",
          "type": "bytes"
        },
        {
          "name": "bump",
          "type": "u8"
        },
        {
          "name": "amount",
          "type": "u64"
        },
        {
          "name": "dstChain",
          "type": "u32"
        },
        {
          "name": "dstAddress",
          "type": "bytes"
        },
        {
          "name": "dstToken",
          "type": "u32"
        },
        {
          "name": "ccmParameters",
          "type": {
            "option": {
              "defined": {
                "name": "ccmParams"
              }
            }
          }
        },
        {
          "name": "cfParameters",
          "type": "bytes"
        }
      ]
    },
    {
      "name": "contractSwapToken",
      "discriminator": [
        95,
        144,
        96,
        179,
        3,
        2,
        75,
        95
      ],
      "accounts": [
        {
          "name": "vault"
        },
        {
          "name": "dataAccount"
        },
        {
          "name": "tokenVaultAssociatedTokenAcount",
          "writable": true
        },
        {
          "name": "pda",
          "pda": {
            "seeds": [
              {
                "kind": "arg",
                "path": "seed"
              }
            ]
          }
        },
        {
          "name": "pdaAssociatedTokenAccount",
          "writable": true
        },
        {
          "name": "tokenSupportedAccount"
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "mint"
        },
        {
          "name": "eventAuthority"
        }
      ],
      "args": [
        {
          "name": "seed",
          "type": "bytes"
        },
        {
          "name": "bump",
          "type": "u8"
        },
        {
          "name": "amount",
          "type": "u64"
        },
        {
          "name": "dstChain",
          "type": "u32"
        },
        {
          "name": "dstAddress",
          "type": "bytes"
        },
        {
          "name": "dstToken",
          "type": "u32"
        },
        {
          "name": "ccmParameters",
          "type": {
            "option": {
              "defined": {
                "name": "ccmParams"
              }
            }
          }
        },
        {
          "name": "cfParameters",
          "type": "bytes"
        },
        {
          "name": "decimals",
          "type": "u8"
        }
      ]
    },
    {
      "name": "initialize",
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  97,
                  116,
                  97
                ]
              }
            ]
          }
        },
        {
          "name": "initializer",
          "writable": true,
          "signer": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "newAggKey",
          "type": "pubkey"
        },
        {
          "name": "newGovKey",
          "type": "pubkey"
        },
        {
          "name": "newTokenVaultPda",
          "type": "pubkey"
        },
        {
          "name": "tokenVaultPdaBump",
          "type": "u8"
        },
        {
          "name": "upgradeSignerPda",
          "type": "pubkey"
        },
        {
          "name": "upgradeSignerPdaBump",
          "type": "u8"
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "dataAccount",
      "discriminator": [
        85,
        240,
        182,
        158,
        76,
        7,
        18,
        233
      ]
    },
    {
      "name": "supportedToken",
      "discriminator": [
        56,
        162,
        96,
        99,
        193,
        245,
        204,
        108
      ]
    }
  ],
  "events": [
    {
      "name": "receivedCcm",
      "discriminator": [
        220,
        233,
        232,
        105,
        128,
        112,
        80,
        63
      ]
    }
  ],
  "types": [
    {
      "name": "ccmParams",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "message",
            "type": "bytes"
          },
          {
            "name": "gasAmount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "dataAccount",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "aggKey",
            "type": "pubkey"
          },
          {
            "name": "govKey",
            "type": "pubkey"
          },
          {
            "name": "tokenVaultPda",
            "type": "pubkey"
          },
          {
            "name": "tokenVaultBump",
            "type": "u8"
          },
          {
            "name": "upgradeSignerPda",
            "type": "pubkey"
          },
          {
            "name": "upgradeSignerPdaBump",
            "type": "u8"
          },
          {
            "name": "suspended",
            "type": "bool"
          },
          {
            "name": "suspendedIxSwaps",
            "type": "bool"
          },
          {
            "name": "suspendedEventSwaps",
            "type": "bool"
          },
          {
            "name": "minNativeSwapAmount",
            "type": "u64"
          },
          {
            "name": "maxDstAddressLen",
            "type": "u16"
          },
          {
            "name": "maxCcmMessageLen",
            "type": "u32"
          },
          {
            "name": "maxCfParametersLen",
            "type": "u32"
          },
          {
            "name": "maxEventAccounts",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "receivedCcm",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "sourceChain",
            "type": "u32"
          },
          {
            "name": "sourceAddress",
            "type": "bytes"
          },
          {
            "name": "message",
            "type": "bytes"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "remainingPubkeys",
            "type": {
              "vec": "pubkey"
            }
          },
          {
            "name": "remainingIsSigner",
            "type": {
              "vec": "bool"
            }
          },
          {
            "name": "remainingIsWritable",
            "type": {
              "vec": "bool"
            }
          }
        ]
      }
    },
    {
      "name": "supportedToken",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "tokenMintPubkey",
            "type": "pubkey"
          },
          {
            "name": "minSwapAmount",
            "type": "u64"
          }
        ]
      }
    }
  ]
};
