/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/vault.json`.
 */
export type Vault = {
  "address": "8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf",
  "metadata": {
    "name": "vault",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Created with Anchor"
  },
  "instructions": [
    {
      "name": "disableTokenSupport",
      "discriminator": [
        72,
        4,
        35,
        144,
        194,
        49,
        71,
        64
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "govKey",
          "writable": true,
          "signer": true
        },
        {
          "name": "tokenSupportedAccount",
          "writable": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "enableTokenSupport",
      "discriminator": [
        125,
        160,
        180,
        50,
        26,
        27,
        112,
        153
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "govKey",
          "writable": true,
          "signer": true
        },
        {
          "name": "tokenSupportedAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  117,
                  112,
                  112,
                  111,
                  114,
                  116,
                  101,
                  100,
                  95,
                  116,
                  111,
                  107,
                  101,
                  110
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ]
          }
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
          "name": "minSwapAmount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "executeCcmNativeCall",
      "discriminator": [
        125,
        5,
        11,
        227,
        128,
        66,
        224,
        178
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "signer": true
        },
        {
          "name": "receiverNative",
          "writable": true
        },
        {
          "name": "cfReceiver",
          "docs": [
            "the aggregate key signature."
          ]
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
      "name": "executeCcmTokenCall",
      "discriminator": [
        108,
        184,
        162,
        123,
        159,
        222,
        170,
        35
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "signer": true
        },
        {
          "name": "receiverTokenAccount",
          "writable": true
        },
        {
          "name": "cfReceiver",
          "docs": [
            "without passing the aggregate key signature."
          ]
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
      "name": "fetchNative",
      "discriminator": [
        142,
        36,
        101,
        143,
        108,
        89,
        41,
        140
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
          "name": "depositChannelPda",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  104,
                  97,
                  110,
                  110,
                  101,
                  108
                ]
              },
              {
                "kind": "arg",
                "path": "seed"
              }
            ]
          }
        },
        {
          "name": "depositChannelHistoricalFetch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  104,
                  105,
                  115,
                  116,
                  95,
                  102,
                  101,
                  116,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "depositChannelPda"
              }
            ]
          }
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
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
        }
      ]
    },
    {
      "name": "fetchTokens",
      "discriminator": [
        73,
        71,
        16,
        100,
        44,
        176,
        198,
        70
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
          "name": "depositChannelPda",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  104,
                  97,
                  110,
                  110,
                  101,
                  108
                ]
              },
              {
                "kind": "arg",
                "path": "seed"
              }
            ]
          }
        },
        {
          "name": "depositChannelAssociatedTokenAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "depositChannelPda"
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
          "name": "mint"
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "depositChannelHistoricalFetch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  104,
                  105,
                  115,
                  116,
                  95,
                  102,
                  101,
                  116,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "depositChannelAssociatedTokenAccount"
              }
            ]
          }
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
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
          "signer": true,
          "address": "HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp"
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
          "name": "expectedTokenVaultPda",
          "type": "pubkey"
        },
        {
          "name": "expectedTokenVaultPdaBump",
          "type": "u8"
        },
        {
          "name": "expectedUpgradeSignerPda",
          "type": "pubkey"
        },
        {
          "name": "expectedUpgradeSignerPdaBump",
          "type": "u8"
        },
        {
          "name": "suspendedVault",
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
    },
    {
      "name": "rotateAggKey",
      "discriminator": [
        78,
        81,
        143,
        171,
        221,
        165,
        214,
        139
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "aggKey",
          "writable": true,
          "signer": true
        },
        {
          "name": "newAggKey",
          "writable": true
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "skipTransferFunds",
          "type": "bool"
        }
      ]
    },
    {
      "name": "setGovKeyWithAggKey",
      "discriminator": [
        66,
        64,
        58,
        40,
        15,
        75,
        215,
        162
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "aggKey",
          "signer": true
        }
      ],
      "args": [
        {
          "name": "newGovKey",
          "type": "pubkey"
        }
      ]
    },
    {
      "name": "setGovKeyWithGovKey",
      "discriminator": [
        251,
        142,
        231,
        255,
        111,
        143,
        165,
        106
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "govKey",
          "signer": true
        }
      ],
      "args": [
        {
          "name": "newGovKey",
          "type": "pubkey"
        }
      ]
    },
    {
      "name": "setProgramSwapsParameters",
      "discriminator": [
        129,
        254,
        31,
        151,
        111,
        149,
        135,
        77
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "govKey",
          "signer": true
        }
      ],
      "args": [
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
    },
    {
      "name": "setSuspendedState",
      "discriminator": [
        145,
        13,
        20,
        161,
        30,
        62,
        226,
        32
      ],
      "accounts": [
        {
          "name": "dataAccount",
          "writable": true
        },
        {
          "name": "govKey",
          "signer": true
        }
      ],
      "args": [
        {
          "name": "suspend",
          "type": "bool"
        },
        {
          "name": "suspendLegacySwaps",
          "type": "bool"
        },
        {
          "name": "suspendEventSwaps",
          "type": "bool"
        }
      ]
    },
    {
      "name": "transferTokens",
      "discriminator": [
        54,
        180,
        238,
        175,
        74,
        85,
        126,
        188
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "signer": true
        },
        {
          "name": "tokenVaultPda"
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
          "name": "toTokenAccount",
          "writable": true
        },
        {
          "name": "mint"
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        },
        {
          "name": "decimals",
          "type": "u8"
        }
      ]
    },
    {
      "name": "transferVaultUpgradeAuthority",
      "discriminator": [
        114,
        247,
        72,
        110,
        145,
        65,
        236,
        153
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "aggKey",
          "signer": true
        },
        {
          "name": "programDataAddress",
          "writable": true
        },
        {
          "name": "programAddress"
        },
        {
          "name": "newAuthority"
        },
        {
          "name": "signerPda",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  105,
                  103,
                  110,
                  101,
                  114
                ]
              }
            ]
          }
        },
        {
          "name": "bpfLoaderUpgradeable",
          "address": "BPFLoaderUpgradeab1e11111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "upgradeProgram",
      "docs": [
        "* ****************************************************************************\n     * *************************** IMPORTANT NOTE *********************************\n     * ****************************************************************************\n     * The signer_pda is the upgrade authority for the vault program. Changing this\n     * logic should be done with caution and understanding the consequences.\n     *\n     *        DO NOT MODIFY THIS WITHOUT UNDERSTANDING THE CONSEQUENCES!\n     * ****************************************************************************\n     * ****************************************************************************"
      ],
      "discriminator": [
        223,
        236,
        39,
        89,
        111,
        204,
        114,
        37
      ],
      "accounts": [
        {
          "name": "dataAccount"
        },
        {
          "name": "govKey",
          "signer": true
        },
        {
          "name": "programDataAddress",
          "writable": true
        },
        {
          "name": "programAddress",
          "writable": true
        },
        {
          "name": "bufferAddress",
          "writable": true
        },
        {
          "name": "spillAddress",
          "writable": true
        },
        {
          "name": "sysvarRent",
          "address": "SysvarRent111111111111111111111111111111111"
        },
        {
          "name": "sysvarClock",
          "address": "SysvarC1ock11111111111111111111111111111111"
        },
        {
          "name": "signerPda",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  105,
                  103,
                  110,
                  101,
                  114
                ]
              }
            ]
          }
        },
        {
          "name": "bpfLoaderUpgradeable",
          "address": "BPFLoaderUpgradeab1e11111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "xSwapNativeLegacy",
      "discriminator": [
        97,
        21,
        117,
        255,
        21,
        6,
        232,
        176
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
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "eventAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  95,
                  95,
                  101,
                  118,
                  101,
                  110,
                  116,
                  95,
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "program"
        }
      ],
      "args": [
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
      "name": "xSwapTokenLegacy",
      "discriminator": [
        248,
        32,
        195,
        34,
        38,
        180,
        117,
        127
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
          "signer": true
        },
        {
          "name": "fromTokenAccount",
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
          "name": "eventAuthority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  95,
                  95,
                  101,
                  118,
                  101,
                  110,
                  116,
                  95,
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "program"
        }
      ],
      "args": [
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
      "name": "depositChannelHistoricalFetch",
      "discriminator": [
        188,
        68,
        197,
        38,
        48,
        192,
        81,
        100
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
      "name": "aggKeyRotated",
      "discriminator": [
        133,
        39,
        145,
        216,
        63,
        154,
        134,
        245
      ]
    },
    {
      "name": "govKeyRotated",
      "discriminator": [
        71,
        44,
        22,
        197,
        63,
        250,
        150,
        83
      ]
    },
    {
      "name": "govKeySetByAggKey",
      "discriminator": [
        135,
        202,
        24,
        202,
        91,
        182,
        141,
        24
      ]
    },
    {
      "name": "govKeySetByGovKey",
      "discriminator": [
        198,
        58,
        153,
        108,
        67,
        162,
        174,
        167
      ]
    },
    {
      "name": "programSwapsParametersSet",
      "discriminator": [
        173,
        82,
        238,
        223,
        74,
        121,
        243,
        142
      ]
    },
    {
      "name": "suspended",
      "discriminator": [
        220,
        179,
        163,
        2,
        249,
        252,
        157,
        102
      ]
    },
    {
      "name": "swapEvent",
      "discriminator": [
        64,
        198,
        205,
        232,
        38,
        8,
        113,
        226
      ]
    },
    {
      "name": "tokenSupportDisabled",
      "discriminator": [
        35,
        252,
        131,
        176,
        50,
        103,
        135,
        13
      ]
    },
    {
      "name": "tokenSupportEnabled",
      "discriminator": [
        11,
        203,
        178,
        141,
        170,
        213,
        67,
        234
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "invalidTokenVaultAccount",
      "msg": "Token Vault pda account does not match the expected program id"
    },
    {
      "code": 6001,
      "name": "suspended",
      "msg": "Vault program is suspended"
    },
    {
      "code": 6002,
      "name": "invalidRemainingAccount",
      "msg": "An invalid account is provided as a remaining account"
    },
    {
      "code": 6003,
      "name": "invalidRemainingAccountSigner",
      "msg": "A remaining account can't be a signer"
    },
    {
      "code": 6004,
      "name": "unchangedSuspendedState",
      "msg": "Suspended state unchanged"
    },
    {
      "code": 6005,
      "name": "invalidSwapParameters",
      "msg": "Invalid swap parameters"
    },
    {
      "code": 6006,
      "name": "invalidTokenVaultBump",
      "msg": "Invalid token vault bump"
    },
    {
      "code": 6007,
      "name": "invalidUpgradeSignerPda",
      "msg": "Invalid upgrade signer pda"
    },
    {
      "code": 6008,
      "name": "invalidUpgradeSignerPdaBump",
      "msg": "Invalid upgrade signer bump"
    },
    {
      "code": 6009,
      "name": "suspendedIxSwaps",
      "msg": "Instruction swaps are suspended"
    },
    {
      "code": 6010,
      "name": "suspendedEventSwaps",
      "msg": "Event swaps are suspended"
    },
    {
      "code": 6011,
      "name": "nativeAmountBelowMinimumSwapAmount",
      "msg": "Native amount below minimum swap amount"
    },
    {
      "code": 6012,
      "name": "tokenAmountBelowMinimumSwapAmount",
      "msg": "Token amount below minimum swap amount"
    },
    {
      "code": 6013,
      "name": "destinationAddressExceedsMaxLength",
      "msg": "Destination address exceeds max length"
    },
    {
      "code": 6014,
      "name": "cfParametersExceedsMaxLength",
      "msg": "Cf Parameters exceeds max length"
    },
    {
      "code": 6015,
      "name": "ccmMessageExceedsMaxLength",
      "msg": "Ccm message exceeds max length"
    }
  ],
  "types": [
    {
      "name": "aggKeyRotated",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldAggKey",
            "type": "pubkey"
          },
          {
            "name": "newAggKey",
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
      "name": "depositChannelHistoricalFetch",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "amount",
            "type": "u128"
          }
        ]
      }
    },
    {
      "name": "govKeyRotated",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldGovKey",
            "type": "pubkey"
          },
          {
            "name": "newGovKey",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "govKeySetByAggKey",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldGovKey",
            "type": "pubkey"
          },
          {
            "name": "newGovKey",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "govKeySetByGovKey",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldGovKey",
            "type": "pubkey"
          },
          {
            "name": "newGovKey",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "programSwapsParametersSet",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldMinSwapAmount",
            "type": "u64"
          },
          {
            "name": "oldMaxDstAddressLen",
            "type": "u16"
          },
          {
            "name": "oldMaxCcmMessageLen",
            "type": "u32"
          },
          {
            "name": "oldMaxCfParametersLen",
            "type": "u32"
          },
          {
            "name": "oldMaxEventAccounts",
            "type": "u32"
          },
          {
            "name": "newMinSwapAmount",
            "type": "u64"
          },
          {
            "name": "newMaxDstAddressLen",
            "type": "u16"
          },
          {
            "name": "newMaxCcmMessageLen",
            "type": "u32"
          },
          {
            "name": "newMaxCfParametersLen",
            "type": "u32"
          },
          {
            "name": "newMaxEventAccounts",
            "type": "u32"
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
      "name": "suspended",
      "type": {
        "kind": "struct",
        "fields": [
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
            "name": "srcToken",
            "type": {
              "option": "pubkey"
            }
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "sender",
            "type": "pubkey"
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
      "name": "tokenSupportDisabled",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldTokenSupported",
            "type": {
              "defined": {
                "name": "supportedToken"
              }
            }
          }
        ]
      }
    },
    {
      "name": "tokenSupportEnabled",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "oldTokenSupported",
            "type": {
              "defined": {
                "name": "supportedToken"
              }
            }
          },
          {
            "name": "newTokenSupported",
            "type": {
              "defined": {
                "name": "supportedToken"
              }
            }
          }
        ]
      }
    }
  ]
};
