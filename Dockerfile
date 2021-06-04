FROM trufflesuite/ganache-cli

ENTRYPOINT ["node", "/app/ganache-core.docker.cli.js", "--mnemonic", "chainflip"]