#!/usr/bin/env bash
# This file contains the functions and definitions for
# the Forest tests. It is meant to be sourced by other scripts and not
# executed directly.

FOREST_PATH="forest"
FOREST_CLI_PATH="forest-cli"
FOREST_WALLET_PATH="forest-wallet"
FOREST_TOOL_PATH="forest-tool"

TMP_DIR=$(mktemp --directory)
LOG_DIRECTORY=$TMP_DIR/logs

export TMP_DIR
export LOG_DIRECTORY
export FOREST_WALLET_PATH

function forest_import_non_calibnet_snapshot {
  echo "Importing a non calibnet snapshot"
  $FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --import-snapshot ./test-snapshots/chain4.car
}

function forest_download_and_import_snapshot {
  echo "Downloading and importing snapshot"
  $FOREST_PATH --chain calibnet --encrypt-keystore false --halt-after-import --height=-200 --auto-download-snapshot
}

function forest_check_db_stats {
  echo "Checking DB stats"
  $FOREST_TOOL_PATH db stats --chain calibnet
}

function forest_query_epoch {
  $FOREST_TOOL_PATH archive info "$1" | grep Epoch | awk '{print $2}'
}

function forest_query_state_roots {
  $FOREST_TOOL_PATH archive info "$1" | grep State-roots | awk '{print $2}'
}

function forest_query_format {
  $FOREST_TOOL_PATH archive info "$1" | grep "CAR format" | awk '{print $3}'
}

function forest_run_node_detached {
  echo "Running forest in detached mode"
  $FOREST_PATH --chain calibnet --encrypt-keystore false --log-dir "$LOG_DIRECTORY" --detach --save-token ./admin_token --track-peak-rss
}

function forest_run_node_stateless_detached {
  CONFIG_PATH="./stateless_forest_config.toml"
  echo "${CONFIG_PATH}"
  echo "Running forest in stateless and detached mode"
  cat <<- EOF > $CONFIG_PATH
		[client]
		data_dir = "/tmp/stateless_forest_data"

		[network]
		listening_multiaddrs = ["/ip4/127.0.0.1/tcp/0"]
	EOF

  $FOREST_PATH --detach --chain calibnet --encrypt-keystore false --config "$CONFIG_PATH" --log-dir "$LOG_DIRECTORY" --save-token ./stateless_admin_token --skip-load-actors --stateless
}

function forest_wait_for_sync {
  echo "Waiting for sync"
  timeout 30m $FOREST_CLI_PATH sync wait
}

function forest_init {
  forest_download_and_import_snapshot
  forest_check_db_stats
  forest_run_node_detached

  ADMIN_TOKEN=$(cat admin_token)
  FULLNODE_API_INFO="$ADMIN_TOKEN:/ip4/127.0.0.1/tcp/2345/http"

  export ADMIN_TOKEN
  export FULLNODE_API_INFO

  forest_wait_for_sync
  forest_check_db_stats
}

function forest_init_stateless {
  forest_run_node_stateless_detached

  ADMIN_TOKEN=$(cat stateless_admin_token)
  FULLNODE_API_INFO="$ADMIN_TOKEN:/ip4/127.0.0.1/tcp/2345/http"

  export ADMIN_TOKEN
  export FULLNODE_API_INFO
}

function forest_print_logs_and_metrics {
  echo "Get and print metrics"
  wget -O metrics.log http://localhost:6116/metrics

  echo "--- Forest STDOUT ---"; cat forest.out
  echo "--- Forest STDERR ---"; cat forest.err
  echo "--- Forest Prometheus metrics ---"; cat metrics.log

  echo "Print forest log files"
  ls -hl "$LOG_DIRECTORY"
  cat "$LOG_DIRECTORY"/*
}

function forest_cleanup {
  if pkill -0 forest 2>/dev/null; then
    forest_print_logs_and_metrics
    $FOREST_CLI_PATH shutdown --force
    timeout 10s sh -c "while pkill -0 forest 2>/dev/null; do sleep 1; done"
  fi
}

function assert_eq {
  local expected="$1"
  local actual="$2"
  local msg="${3-}"

  if [ "$expected" == "$actual" ]; then
    return 0
  else
    [ "${#msg}" -gt 0 ] && echo "$expected == $actual :: $msg"
    return 1
  fi
}

trap forest_cleanup EXIT
