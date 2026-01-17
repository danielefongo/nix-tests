{ lib, ... }:
let
  executeCheck =
    checkName: result:
    if builtins.isBool result then
      {
        name = checkName;
        success = result;
      }
    else if builtins.isString result then
      {
        name = checkName;
        error = result;
        success = false;
      }
    else
      throw "Check must return either boolean or string, got: ${builtins.typeOf result}";

  checkEq =
    expected: actual:
    if actual == expected then
      true
    else
      "Expected: ${builtins.toJSON expected}\nGot: ${builtins.toJSON actual}";

  checkNotNull = actual: if actual != null then true else "Expected: not null\nGot: null";

  checkHasAttr =
    attrName: attrSet:
    if attrSet ? ${attrName} then
      true
    else
      "Expected: attribute '${attrName}' to exist\nGot: ${builtins.toJSON (builtins.attrNames attrSet)}";

  checkHasNotAttr =
    attrName: attrSet:
    if attrSet ? ${attrName} then
      "Expected: attribute '${attrName}' to not exist\nGot: ${builtins.toJSON (builtins.attrNames attrSet)}"
    else
      true;

  mkHelpers =
    pos:
    let
      check =
        checkName: checkLambda: actual:
        executeCheck checkName (checkLambda actual);
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

  runTest =
    path: spec:
    let
      pos = builtins.unsafeGetAttrPos "checks" spec;
      location = if pos != null then "${pos.file}:${toString pos.line}" else "unknown";
      checks = spec.checks (mkHelpers pos) spec.context;
      success = builtins.all (c: c.success) checks;
    in
    {
      inherit
        checks
        location
        path
        success
        ;
    };

  flattenTests =
    pathPrefix: item:
    if item ? tests then
      lib.concatMap (child: flattenTests (pathPrefix ++ [ item.name ]) child) item.tests
    else
      [ (runTest (pathPrefix ++ [ item.name ]) item.spec) ];
in
{
  test = name: spec: {
    inherit name spec;
  };

  group = name: tests: {
    inherit name tests;
  };

  runTests = tests: {
    tests = lib.concatMap (item: flattenTests [ ] item) tests;
  };
}
