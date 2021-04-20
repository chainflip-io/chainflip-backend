# Playbooks

The `chainflip-backend` repo uses `ansible` to configure the host machines. In this folder are the playbooks used to run all tasks related but not limited to provisioning new hosts for our staging network.

### `.ansible/run`
This playbook should be run ad-hoc and only from an administrators machine. Primary purpose to update the staging `state-chain`.  
1. Install nginx 
2. Add the relevant ssh keys to all nodes.
3. Sync the nodes to get the latest version of the state-chain-node binary.
4. Purge the state-chain
5. Set up `alice` and `bob` on different machines and make sure they connect to one another on boot.

### `.ansible/staging`
This should only be run in the CI. Primary purpose is to copy the build from the ci and place it in the `/root/state-chain/releases` folder.

### `.ansible/upload`
Not yet used. In time it will be the playbook that uploads new `.deb` files to our debian repo for easy distribution to our validators.
