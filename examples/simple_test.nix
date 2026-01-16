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
