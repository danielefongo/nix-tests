# nix-tests

![Nix](https://img.shields.io/badge/Nix-5277C3?style=flat-square&logo=nix&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)

A lightweight testing framework for Nix, written in Rust.

## Features

- Simple test syntax in pure Nix
- Simple and extensible assertion library
- Test grouping support
- Clear output
- Fast parallel test execution

## Installation

> **Note:** `nix-tests` requires either `rg` (ripgrep, preferred for performance) or `find` to be available in your system. It will automatically use `rg` if available, otherwise fallback to `find`.

### Using Nix Flakes (devShell)

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    nix-tests.url = "github:nixos/danielefongo/nix-tests";
  };

  outputs = { nixpkgs, nix-tests, ... }: {
    devShells.x86_64-linux.default = nixpkgs.legacyPackages.x86_64-linux.mkShell {
      packages = [ nix-tests.packages.x86_64-linux.default ];
    };
  };
}
```

### Using Nix Flakes (overlay)

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    nix-tests.url = "github:danielefongo/nix-tests";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    inputs@{
      nixpkgs,
      nix-tests,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ nix-tests.overlays.default ];
        };
      in
      {
        packages.default = pkgs.nix-tests;
      }
    );
}
```

## Quick Start

Create a test file `example_test.nix`:

```nix
{
  pkgs ? import <nixpkgs> { },
  nix-tests,
}:
let
  lib = pkgs.lib;

  # Custom checks
  isEven = x: if lib.mod x 2 == 0 then true else "${builtins.toString x} is not even";
  longerThan =
    n: s: if builtins.stringLength s > n then true else "'${s}' is not longer than ${toString n}";
in
nix-tests.runTests {
  "random tests" = {
    context = {
      num = 42;
      name = "Alice";
    };
    checks = helpers: ctx: {
      "number equals 42" = helpers.isEq ctx.num 42;
      "name is Alice" = helpers.isTrue (ctx.name == "Alice");
      "name is not Bob" = helpers.isFalse (ctx.name == "Bob");
      "null check" = helpers.isNull null;
      "not null check" = helpers.isNotNull ctx.name;
      "has num attribute" = helpers.hasAttr "num" ctx;
      "no age attribute" = helpers.hasNotAttr "age" ctx;

      # Custom checks
      "is even" = helpers.check isEven ctx.num;
      "long name" = helpers.check (longerThan 3) ctx.name;
      "is less than 100" = helpers.check (
        x: if x < 100 then true else "${toString x} is not less than 100"
      ) ctx.num;
    };
  };
}
```

Run tests:

```bash
# Run all tests in current directory
nix-tests

# Run tests in specific directory
nix-tests ./tests

# Run specific test file
nix-tests example_test.nix

# Run multiple files/directories
nix-tests tests/unit tests/integration specific_test.nix
```

> **Note:** Additional options are available. Run `nix-tests --help` to see all CLI options.

## Configuration

You can create a `.nix-tests.toml` file in your project. Use `nix-tests --show` to see the default configuration.

### CLI Options

- `--help` - Show help message with all available options
- `--config <PATH>` - Specify a custom config directory or file
- `--show` - Display the loaded configuration and exit
- Other CLI options (`--num-threads`, `--format`, etc.) match the TOML config names and override the loaded configuration

### Config Discovery

- Without `--config`: uses default values
- With `--config <PATH>`: searches for `.nix-tests.toml` in the specified directory and parent directories (stopping at `flake.lock`, `.git`, or `/`), or uses the file directly if it's a `.toml` file
- If no config file is found, default values are used

## Limitations

- **Execution Time Granularity**: Execution time (`elapsed`) can only be measured at the file level, not for individual tests. This is because all tests in a file are evaluated together by a single `nix-instantiate` process, and timing is measured externally.

## API

### Test Structure

Tests are defined using an attribute set structure:

```nix
nix-tests.runTests {
  "test name" = {
    context = _: { /* test data */ }; # optional
    checks = helpers: ctx: {
      "check name" = helpers.assertion value;
    };
  };

  "group name" = {
    "nested test" = {
      context = _: { };
      checks = helpers: ctx: { /* ... */ };
    };
  };
}
```

### Checks

All available via `helpers` parameter in `checks`:

- `isEq actual expected` - Assert equality
- `isTrue value` - Assert true
- `isFalse value` - Assert false
- `isNull value` - Assert null
- `isNotNull value` - Assert not null
- `hasAttr attrName attrset` - Assert attribute exists
- `hasNotAttr attrName attrset` - Assert attribute does not exist
- `check checkFn actual` - Generic check

#### Custom checks

Create custom checks by defining functions that return `true` for success or an error message string for failure:

```nix
# Custom checks examples
isEven = x: if lib.mod x 2 == 0 then true else "${builtins.toString x} is not even";
longerThan = n: s: if builtins.stringLength s > n then true else "'${s}' is not longer than ${toString n}";

# Usage in tests
checks = helpers: ctx: {
  "is even" = helpers.check isEven ctx.num;
  "long name" = helpers.check (longerThan 3) ctx.name;
  "is less than 100" = helpers.check (
    x: if x < 100 then true else "${toString x} is not less than 100"
  ) ctx.num;
};
```
