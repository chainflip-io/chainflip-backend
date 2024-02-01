# Add a global variable to control the behavior of the mock
SERVICE_STATUS="active"

systemctl() {
    if [ "$1" = "is-active" ] && [ "$3" = "chainflip-node" ]; then
        if [ "$SERVICE_STATUS" = "active" ]; then
            return 0
        else
            return 1
        fi
    fi

    if [ "$1" = "stop" ] && [ "$2" = "chainflip-node" ]; then
        return 0  # assume it's successful
    fi

    echo "Unhandled systemctl command: $@"
}

# Mock for chainflip-node
chainflip-node() {
    if [ "$1" = "-V" ]; then
        echo "$VERSION_OUTPUT"
    fi
}
