/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/oracle_query_helper.json`.
 */
export type OracleQueryHelper = {
  "address": "GXn7uzbdNgozXuS8fEbqHER1eGpD9yho7FHTeuthWU8z",
  "metadata": {
    "name": "oracleQueryHelper",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Created with Anchor"
  },
  "instructions": [
    {
      "name": "queryPriceFeeds",
      "discriminator": [
        39,
        82,
        53,
        78,
        23,
        38,
        62,
        195
      ],
      "accounts": [
        {
          "name": "chainlinkProgram"
        }
      ],
      "args": []
    }
  ]
};
