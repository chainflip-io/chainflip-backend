# Generate Genesis Key Databases

When a network is started, the nodes require an aggregate key to sign with. This aggregate key, and the secret shares that correspond to this aggregate key need to be generated offline, and assigned to the respective genesis nodes.

This script is used to generate databases for use in the genesis of a testnet.

## Usage

The script takes a CSV file containing the node names and ids, *without spaces*, where the first line contains the headers.

```csv
node_name,node_id,
bashful,5DJVVEYPDFZjj9JtJRE2vGvpeSnzBAUA74VXPSpkGKhJSHbN
doc,5HEezwP9EediVA3s7UqkWKhxqTBwUuYgx3jCcqKV2jB79Fpy
...
```

This CSV file can contain as many nodes as desired for genesis.

Before running the script, you must set the file path of the CSV file that will be used to read in the node names and ids.

```bash
export GENESIS_NODE_IDS="/path/to/csv/file"
```

Now if the CSV is valid, and the path to the CSV is correct, you should be able to run the script.

```bash
./generate-genesis-keys
```

This outputs a DB for each entry in the CSV, using `node_name` as the db's file name. 

For example:
```
bashful.db
doc.db
...
```