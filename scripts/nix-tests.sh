#!/usr/bin/env bash
set -euo pipefail

run_test() {
  local test_file="$1"
  echo "Testing: $test_file"
  if nix-instantiate --eval --strict --json "$test_file" \
    --arg nix-tests "import $NIX_TESTS_LIB_PATH { lib = (import <nixpkgs> {}).lib; }" \
    -A result 2>&1 | sed 's/^trace: //' | grep -v '^true$'; then
    echo "PASS: $test_file"
    return 0
  else
    echo "FAIL: $test_file"
    return 1
  fi
}

run_tests() {
  local args=("$@")

  if [ ${#args[@]} -eq 0 ]; then
    args=(".")
  fi

  local test_files=()
  for arg in "${args[@]}"; do
    if [ -d "$arg" ]; then
      mapfile -t dir_files < <(find "$arg" -name "*_test.nix" -type f)
      test_files+=("${dir_files[@]}")
    elif [ -f "$arg" ]; then
      if [[ "$arg" =~ _test\.nix$ ]]; then
        test_files+=("$arg")
      else
        echo "Skipping non-test file: $arg"
      fi
    else
      echo "File not found, skipping: $arg"
    fi
  done

  if [ ${#test_files[@]} -eq 0 ]; then
    echo "No test files found"
    return 0
  fi

  echo "Found ${#test_files[@]} test file(s)"
  echo ""

  for test_file in "${test_files[@]}"; do
    if ! run_test "$test_file"; then
      return 1
    fi
    echo ""
  done

  return 0
}

run_tests "$@"
