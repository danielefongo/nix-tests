{ ... }:
let
  concatMap = f: list: builtins.concatLists (map f list);
  all = pred: list: builtins.all pred list;

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

  helpers =
    let
      check = checkFn: actual: {
        _checkFn = checkFn;
        _actual = actual;
      };
    in
    {
      inherit check;
      isEq = actual: expected: check (checkEq expected) actual;
      isTrue = actual: check (checkEq true) actual;
      isFalse = actual: check (checkEq false) actual;
      isNull = actual: check (checkEq null) actual;
      isNotNull = actual: check checkNotNull actual;
      hasAttr = attrName: attrSet: check (checkHasAttr attrName) attrSet;
      hasNotAttr = attrName: attrSet: check (checkHasNotAttr attrName) attrSet;
    };

  getLocation = pos: if pos != null then "${pos.file}:${toString pos.line}" else "unknown";

  sortByLine =
    attrs:
    let
      names = builtins.attrNames attrs;
      withPos = map (name: {
        inherit name;
        line = (builtins.unsafeGetAttrPos name attrs).line;
      }) names;
      sorted = builtins.sort (a: b: a.line < b.line) withPos;
    in
    map (x: x.name) sorted;

  runCheck =
    checkDefs: name:
    let
      checkDef = checkDefs.${name};
      checkResult = checkDef._checkFn checkDef._actual;

      success =
        if builtins.isBool checkResult then
          checkResult
        else if builtins.isString checkResult then
          false
        else
          throw "Check must return either boolean or string, got: ${builtins.typeOf checkResult}";
      failure = if builtins.isString checkResult then checkResult else null;
    in
    {
      inherit
        name
        success
        failure
        ;
    };

  runTest =
    path: spec:
    let
      location = getLocation (builtins.unsafeGetAttrPos "checks" spec);

      checkDefs = spec.checks helpers spec.context;
      checks = map (runCheck checkDefs) (sortByLine checkDefs);
      success = all (c: c.success) checks;
    in
    {
      inherit
        path
        location
        success
        checks
        ;
    };

  isTest = value: builtins.isAttrs value && value ? checks;

  flattenTests =
    pathPrefix: attrs:
    concatMap (
      name:
      let
        value = attrs.${name};
        newPath = pathPrefix ++ [ name ];
      in
      if isTest value then [ (runTest newPath value) ] else flattenTests newPath value
    ) (sortByLine attrs);
in
{
  runTests =
    tests:
    let
      result = {
        tests = flattenTests [ ] tests;
      };
    in
    builtins.deepSeq result result;
}
