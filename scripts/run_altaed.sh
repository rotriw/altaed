#!/bin/sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export ALTAED_DATA_DIR="${ALTAED_DATA_DIR:-$project_dir/.cache/ide-session}"
exec "$project_dir/target/debug/altaed" "$@"
