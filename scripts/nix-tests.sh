#!/usr/bin/env bash
set -eo pipefail

run_test() {
  local test_file="$1"
  echo "Testing: $test_file"

  local output
  output=$(nix-instantiate --eval --strict "$test_file" \
    --arg nix-tests "import $NIX_TESTS_LIB_PATH { lib = (import <nixpkgs> {}).lib; }" \
    -A result 2>&1)

  echo "$output" | sed 's/^trace: //' | grep -v '^[0-9]\+$'

  local failed_tests
  failed_tests=$(echo "$output" | grep '^[0-9]\+$' | head -n 1)

  if [ "$failed_tests" = "0" ]; then
    echo "PASS: $test_file"
  else
    echo "FAIL: $test_file (failed test(s): $failed_tests)"
  fi

  return "$failed_tests"
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

  local failed_count=0
  set +e
  for test_file in "${test_files[@]}"; do
    local test_result
    run_test "$test_file"
    test_result=$?
    failed_count=$((failed_count + test_result))
    echo ""
  done
  set -e

  if [ $failed_count -gt 0 ]; then
    echo "$failed_count test(s) failed"
  else
    echo "All tests passed"
  fi

  return $failed_count
}

run_tests "$@"
