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
  "random tests" = helpers: rec {
    ctx = {
      num = 42;
      name = "Alice";
    };

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
}
