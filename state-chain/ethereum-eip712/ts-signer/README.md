# EIP-712 TypeScript Hasher

A standalone TypeScript utility for computing EIP-712 signing hashes, designed to be used as a reference implementation for testing Rust EIP-712 encoding.

Supports three popular JavaScript libraries:

- **ethers.js** (v6) - Most widely used
- **viem** - Modern alternative, rapidly growing adoption
- **@metamask/eth-sig-util** - MetaMask's internal signing library

## Building

Requires [Bun](https://bun.sh/) to be installed.

```bash
bun install
bun run build
```

This creates standalone executables for both platforms:

- `dist/eip712-signer-linux-x64` - Linux x64
- `dist/eip712-signer-darwin-arm64` - macOS ARM64

These can be run without Bun installed.

## Usage

```bash
eip712-signer --library <ethers|viem|eth-sig-util>
```

The utility reads EIP-712 typed data JSON from stdin and outputs the signing hash to stdout.

### Input Format

Standard EIP-712 typed data:

```json
{
  "types": {
    "EIP712Domain": [
      { "name": "name", "type": "string" },
      { "name": "version", "type": "string" },
      { "name": "chainId", "type": "uint256" }
    ],
    "Mail": [
      { "name": "from", "type": "string" },
      { "name": "to", "type": "string" },
      { "name": "message", "type": "string" }
    ]
  },
  "primaryType": "Mail",
  "domain": {
    "name": "Example",
    "version": "1",
    "chainId": 1
  },
  "message": {
    "from": "alice@example.com",
    "to": "bob@example.com",
    "message": "Hello!"
  }
}
```

### Output Format

On success:

```json
{
  "library": "ethers",
  "signingHash": "0x..."
}
```

On error:

```json
{
  "error": "Error message"
}
```

### Example

```bash
echo '{"types":{...},"primaryType":"Mail","domain":{...},"message":{...}}' | ./dist/eip712-signer-darwin-arm64 --library ethers
```
