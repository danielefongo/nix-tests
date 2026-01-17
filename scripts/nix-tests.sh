#!/usr/bin/env bash
set -eo pipefail

run_test() {
  local test_file="$1"
  echo "Testing: $test_file"

  local output
  output=$(nix-instantiate --eval --strict "$test_file" \
    --arg nix-tests "import $NIX_TESTS_LIB_PATH { lib = (import <nixpkgs> {}).lib; }" \
    -A result 2>&1)

  echo "$output" | sed 's/^trace: //' | rg -v '^[0-9]+$'

  local failed_tests
  failed_tests=$(echo "$output" | rg '^[0-9]+$' | head -n 1)

  if [ "$failed_tests" = "0" ]; then
    echo "PASS: $test_file"
  else
    echo "FAIL: $test_file (failed test(s): $failed_tests)"
  fi

  return "${failed_tests:-0}"
}

run_tests() {
  local args=("$@")

  if [ ${#args[@]} -eq 0 ]; then
    args=(".")
  fi

  local test_files=()
  mapfile -t test_files < <(rg --files --glob "*_test.nix" "${args[@]}" | grep -E '_test\.nix$' | awk '!seen[$0]++' 2>/dev/null || true)

  for arg in "${args[@]}"; do
    if [[ -f "$arg" && ! "$arg" =~ _test\.nix$ ]]; then
      echo "Warning: '$arg' is not a test file, skipping."
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
