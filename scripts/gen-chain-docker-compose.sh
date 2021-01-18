#!/bin/bash


### Set up temporary file area
TEMPLATE_DIR="`dirname $0`/docker-compose-templates"
TEMP_DIR=`mktemp -d`

function cleanup {
    rm -rf "${TEMP_DIR}"
}

# Make sure we clean up the temporary files on exit.
trap cleanup EXIT


### Interpolate variables in templates and append to the docker-compose command.
CMD="docker-compose --project-directory $0/../../."

# The base IP address and suffix.
SUBNET_BASE="172.28.0"
NETWORK_SUFFIX=2

# Base configuration file.
BASE_CONFIG_FILE=${TEMP_DIR}/state-chain-base.yml

sed -e "s/\${SUBNET_BASE}/${SUBNET_BASE}/" \
    ${TEMPLATE_DIR}/state-chain-base.template.yml \
> ${BASE_CONFIG_FILE}

CMD="${CMD} -f ${BASE_CONFIG_FILE}"

# Node configuration files.
# Names should be "alice", "bob", etc.
for NODE_NAME in $@
do
    NODE_CONFIG_FILE=${TEMP_DIR}/state-chain-node-${NODE_NAME}.yml

    # Substitute the args into the template and save to the temporary file.
    sed -e "s/\${SUBNET_BASE}/${SUBNET_BASE}/" \
        -e "s/\${NODE_NAME}/${NODE_NAME}/" \
        -e "s/\${NETWORK_SUFFIX}/${NETWORK_SUFFIX}/" \
        ${TEMPLATE_DIR}/state-chain-node.template.yml \
    > ${NODE_CONFIG_FILE}

    # Append the file to the command using -f.
    CMD="${CMD} -f ${NODE_CONFIG_FILE}"
    
    # Increment the IP address suffix.
    NETWORK_SUFFIX=$(($NETWORK_SUFFIX + 1))
done

# The docker-compose 'config' command validates and combines the files into a single file.
CMD="${CMD} config --no-interpolate"

### Run the command. This will output the generated file to stdout.

$CMD
