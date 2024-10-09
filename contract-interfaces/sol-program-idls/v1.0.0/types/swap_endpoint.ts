/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/swap_endpoint.json`.
 */
export type SwapEndpoint = {
  "address": "35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT",
  "metadata": {
    "name": "swapEndpoint",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Created with Anchor"
  },
  "instructions": [
    {
      "name": "closeEventAccounts",
      "discriminator": [
        165,
        102,
        61,
        1,
        185,
        77,
        189,
        121
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "writable": true,
          "signer": true
        },
        {
          "name": "swapEndpointDataAccount",
          "writable": true
        }
      ],
      "args": []
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
          "name": "swapEndpointDataAccount",
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
          "name": "signer",
          "writable": true,
          "signer": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "xSwapNative",
      "discriminator": [
        163,
        38,
        92,
        226,
        243,
        105,
        141,
        196
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "writable": true
        },
        {
          "name": "from",
          "writable": true,
          "signer": true
        },
        {
          "name": "eventDataAccount",
          "writable": true,
          "signer": true
        },
        {
          "name": "swapEndpointDataAccount",
          "writable": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "swapNativeParams",
          "type": {
            "defined": {
              "name": "swapNativeParams"
            }
          }
        }
      ]
    },
    {
      "name": "xSwapToken",
      "discriminator": [
        69,
        50,
        252,
        99,
        229,
        83,
        119,
        235
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "tokenVaultAssociatedTokenAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "data_account.token_vault_pda",
                "account": "dataAccount"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "from",
          "writable": true,
          "signer": true
        },
        {
          "name": "fromTokenAccount",
          "writable": true
        },
        {
          "name": "eventDataAccount",
          "writable": true,
          "signer": true
        },
        {
          "name": "swapEndpointDataAccount",
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
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "swapTokenParams",
          "type": {
            "defined": {
              "name": "swapTokenParams"
            }
          }
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
    },
    {
      "name": "swapEndpointDataAccount",
      "discriminator": [
        79,
        152,
        191,
        225,
        128,
        108,
        11,
        139
      ]
    },
    {
      "name": "swapEvent",
      "discriminator": [
        150,
        251,
        114,
        94,
        200,
        113,
        248,
        70
      ]
    }
  ],
  "events": [
    {
      "name": "cantDeserializeEventAccount",
      "discriminator": [
        248,
        26,
        198,
        175,
        8,
        58,
        229,
        137
      ]
    },
    {
      "name": "eventAccountNotTracked",
      "discriminator": [
        202,
        29,
        253,
        107,
        20,
        196,
        36,
        210
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "mathOverflow",
      "msg": "Overflow in arithmetic operation"
    },
    {
      "code": 6001,
      "name": "mathUnderflow",
      "msg": "Underflow in arithmetic operation"
    }
  ],
  "types": [
    {
      "name": "cantDeserializeEventAccount",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "eventAccount",
            "type": "pubkey"
          },
          {
            "name": "payee",
            "type": "pubkey"
          }
        ]
      }
    },
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
      "docs": [
        "* ****************************************************************************\n * *************************** IMPORTANT NOTE *********************************\n * ****************************************************************************\n * If the vault is upgraded and the DataAccount struct is modified we need to\n * check the compatibility and ensure there is a proper migration process, given\n * that the Vault bytecode is the only thing being upgraded, not the data account.\n *\n * The easiest approach on upgrade is keeping the DataAccount unchanged and use\n * a new account struct for any new data that is required.\n *\n *        DO NOT MODIFY THIS WITHOUT UNDERSTANDING THE CONSEQUENCES!\n * ****************************************************************************\n * ****************************************************************************"
      ],
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
            "name": "suspendedLegacySwaps",
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
      "name": "eventAccountNotTracked",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "eventAccount",
            "type": "pubkey"
          },
          {
            "name": "payee",
            "type": "pubkey"
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
    },
    {
      "name": "swapEndpointDataAccount",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "historicalNumberEventAccounts",
            "type": "u128"
          },
          {
            "name": "openEventAccounts",
            "type": {
              "vec": "pubkey"
            }
          }
        ]
      }
    },
    {
      "name": "swapEvent",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "sender",
            "type": "pubkey"
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
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "srcToken",
            "type": {
              "option": "pubkey"
            }
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
      }
    },
    {
      "name": "swapNativeParams",
      "type": {
        "kind": "struct",
        "fields": [
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
      }
    },
    {
      "name": "swapTokenParams",
      "type": {
        "kind": "struct",
        "fields": [
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
      }
    }
  ]
};
