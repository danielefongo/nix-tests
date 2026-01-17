#!/usr/bin/env bash
set -eo pipefail
OUTPUT_FORMAT="human"

run_test() {
  local test_file="$1"
  local output
  local exit_code=0

  output=$(nix-instantiate --eval --strict --json "$test_file" \
    --arg nix-tests "import $NIX_TESTS_LIB_PATH { lib = (import <nixpkgs> {}).lib; }" \
    -A result 2>&1) || exit_code=$?

  if [ "$exit_code" -ne 0 ]; then
    jq -c -n --arg file "$test_file" --arg failure "$output" '{ file: $file, failure: $failure }'
    return 0
  fi

  local file_path
  file_path=$(echo "$output" | jq -r '.tests[0].location | split(":")[0]')
  echo "$output" | jq -c --arg file "$file_path" '. + {file: $file}'
}

json_to_human() {
  jq -r '
    (.tests[0].location | split(":")[0]) as $file |

    "Testing: \($file)",

    (.tests[] |
      .path as $path |
      .location as $test_location |
      .checks[] |
      (if .success then "✓" else "✗" end) + " " + ($path | join(" -> ")) + " -> " + .name +
      (if .success == false then
        if .error then
          "\n    Error:\n" +
          (.error | split("\n") | map("      " + .) | join("\n")) +
          "\n      at \($test_location)"
        else
          "\n    Failed at \($test_location)"
        end
      else "" end)
    ),

    if all(.tests[]; .success) then
      "PASSED"
    else
      ([.tests[] | .checks[] | select(.success == false)] | length) as $failed |
      "FAILED (\($failed) failed)"
    end
  '
}

handle_test_error() {
  local error_json="$1"
  local file
  local failure

  file=$(echo "$error_json" | jq -r '.file')
  failure=$(echo "$error_json" | jq -r '.failure')

  echo "ERROR in $file:"
  echo "$failure"
}

run_tests_human() {
  local test_files=("$@")

  if [ ${#test_files[@]} -eq 0 ]; then
    echo "No test files found"
    return 0
  fi

  echo "Found ${#test_files[@]} test file(s)"
  echo ""

  local failed_count=0
  local error_count=0
  set +e
  for test_file in "${test_files[@]}"; do
    local json_output
    json_output=$(run_test "$test_file")

    if echo "$json_output" | jq -e '.failure' >/dev/null 2>&1; then
      handle_test_error "$json_output"
      error_count=$((error_count + 1))
      echo ""
      continue
    fi

    jq -n --argjson data "$json_output" '{tests: $data.tests}' | json_to_human

    if echo "$json_output" | jq -e 'any(.tests[]; .success == false)' >/dev/null 2>&1; then
      failed_count=$((failed_count + 1))
    fi

    echo ""
  done
  set -e

  local total_issues=$((failed_count + error_count))

  if [ $total_issues -eq 0 ]; then
    echo "All tests passed"
    return 0
  fi

  [ $error_count -gt 0 ] && echo "$error_count file(s) had errors"
  [ $failed_count -gt 0 ] && echo "$failed_count file(s) failed"
  return 1
}

run_tests_json() {
  local test_files=("$@")

  if [ ${#test_files[@]} -eq 0 ]; then
    return 0
  fi

  local has_failures=false
  set +e
  for test_file in "${test_files[@]}"; do
    local json_output
    json_output=$(run_test "$test_file")

    echo "$json_output"

    if echo "$json_output" | jq -e '.failure or any(.tests[]?; .success == false)' >/dev/null; then
      has_failures=true
    fi
  done
  set -e

  if [ "$has_failures" = true ]; then
    return 1
  fi
}

run_tests() {
  local args=()

  for arg in "$@"; do
    if [[ "$arg" == "--json" ]]; then
      OUTPUT_FORMAT="json"
    else
      args+=("$arg")
    fi
  done

  if [ ${#args[@]} -eq 0 ]; then
    args=(".")
  fi

  local test_files=()
  mapfile -t test_files < <(rg --files --glob "*_test.nix" "${args[@]}" | grep -E '_test\.nix$' | awk '!seen[$0]++' 2>/dev/null || true)

  if [[ "$OUTPUT_FORMAT" == "json" ]]; then
    run_tests_json "${test_files[@]}"
  else
    for arg in "${args[@]}"; do
      if [[ -f "$arg" && ! "$arg" =~ _test\.nix$ ]]; then
        echo "Warning: '$arg' is not a test file, skipping."
      fi
    done

    run_tests_human "${test_files[@]}"
  fi
}

run_tests "$@"
