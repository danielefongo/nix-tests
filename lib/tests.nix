{ lib, ... }:
let
  executeCheck =
    testName: checkName: result: pos:
    let
      fullName = "${testName} -> ${checkName}";
      location = if pos != null then "\n  at ${pos.file}:${toString pos.line}" else "";
    in
    if builtins.isBool result then
      if result then
        builtins.trace " ${fullName}" true
      else
        builtins.trace " ${fullName}\n  Check failed${location}" false
    else if builtins.isString result then
      builtins.trace " ${fullName}\n  ${result}${location}" false
    else
      throw "Check must return either boolean or string, got: ${builtins.typeOf result}";

  checkEq =
    expected: actual:
    if actual == expected then
      true
    else
      "Expected: ${builtins.toJSON expected}\n  Got:      ${builtins.toJSON actual}";

  checkNotNull = actual: if actual != null then true else "Expected: not null\n  Got: null";

  checkHasAttr =
    attrName: attrSet:
    if attrSet ? ${attrName} then
      true
    else
      "Expected: attribute '${attrName}' to exist\n  Got: ${builtins.toJSON (builtins.attrNames attrSet)}";

  checkHasNotAttr =
    attrName: attrSet:
    if attrSet ? ${attrName} then
      "Expected: attribute '${attrName}' to not exist\n  Got: ${builtins.toJSON (builtins.attrNames attrSet)}"
    else
      true;

  mkHelpers =
    testName: pos:
    let
      check =
        checkName: checkLambda: actual:
        executeCheck testName checkName (checkLambda actual) pos;
    in
    {
      inherit check;
      isEq =
        checkName: actual: expected:
        check checkName (checkEq expected) actual;
      isTrue = checkName: actual: check checkName (checkEq true) actual;
      isFalse = checkName: actual: check checkName (checkEq false) actual;
      isNull = checkName: actual: check checkName (checkEq null) actual;
      isNotNull = checkName: actual: check checkName checkNotNull actual;
      hasAttr =
        checkName: attrName: attrSet:
        check checkName (checkHasAttr attrName) attrSet;
      hasNotAttr =
        checkName: attrName: attrSet:
        check checkName (checkHasNotAttr attrName) attrSet;
    };

  mkTest =
    testName: arg:
    let
      helpers = mkHelpers testName (builtins.unsafeGetAttrPos "checks" arg);
      context = arg.context or { };
      checksFn = arg.checks or (_: _: [ ]);
    in
    {
      name = testName;
      inherit context;
      checksFn = checksFn;
      checks = checksFn helpers context;
    };
in
{
  test = mkTest;

  group =
    groupName: tests:
    map (
      testDef:
      mkTest "${groupName} -> ${testDef.name}" {
        inherit (testDef) context checksFn;
        checks = testDef.checksFn;
      }
    ) tests;

  runTests =
    tests:
    let
      flattenTests =
        list: lib.concatMap (item: if builtins.isList item then flattenTests item else [ item ]) list;
      allTests = flattenTests tests;
      allChecks = lib.concatLists (map (test: test.checks) allTests);
      failedCount = builtins.length (builtins.filter (x: x == false) allChecks);
    in
    failedCount;
}
