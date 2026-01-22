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
{
  result = nix-tests.runTests [
    (nix-tests.test "random tests" {
      context = {
        num = 42;
        name = "Alice";
      };
      checks = helpers: ctx: [
        (helpers.isEq "number equals 42" ctx.num 42)
        (helpers.isTrue "name is Alice" (ctx.name == "Alice"))
        (helpers.isFalse "name is not Bob" (ctx.name == "Bob"))
        (helpers.isNull "null check" null)
        (helpers.isNotNull "not null check" ctx.name)
        (helpers.hasAttr "has num attribute" "num" ctx)
        (helpers.hasNotAttr "no age attribute" "age" ctx)

        # Custom checks
        (helpers.check "is even" isEven ctx.num)
        (helpers.check "long name" (longerThan 3) ctx.name)
        (helpers.check "is less than 100" (
          x: if x < 100 then true else "${toString x} is not less than 100"
        ) ctx.num)
      ];
    })
  ];
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

When a test fails, the error pointer indicates the entire `checks` block rather than the specific individual check that failed.

## API

### Core Functions

- `test name { context, checks }` - Create a test case
- `group name tests` - Group multiple tests
- `runTests tests` - Execute tests (returns true or throws)

### Checks

All available via `helpers` parameter in `checks`:

- `isEq name actual expected` - Assert equality
- `isTrue name value` - Assert true
- `isFalse name value` - Assert false
- `isNull name value` - Assert null
- `isNotNull name value` - Assert not null
- `hasAttr name attrName attrset` - Assert attribute exists
- `hasNotAttr name attrName attrset` - Assert attribute does not exist
- `check name checkFn actual` - Generic check

#### Custom checks

Create custom checks using the `helpers.check` function or by defining check functions that return `true` for success or an error message string for failure:

```nix
# Custom assertions examples
isEven = x: if lib.mod x 2 == 0 then true else "${builtins.toString x} is not even";
longerThan = n: s: if builtins.stringLength s > n then true else "'${s}' is not longer than ${toString n}";

# Usage in tests
(helpers.check "is even" isEven ctx.num)
(helpers.check "long name" (longerThan 3) ctx.name)
(helpers.check "is less than 100" (
  x: if x < 100 then true else "${toString x} is not less than 100"
) ctx.num)
```
