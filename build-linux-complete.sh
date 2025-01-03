#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

PROFILE="$1"

set -a
source ".$PROFILE.env"
set +a

# Build commands
cargo build --features "playtime,licensing"