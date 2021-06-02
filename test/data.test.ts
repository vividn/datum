import { beforeEach, describe, expect, it, jest } from "@jest/globals";
import { parseData, parseDataType } from "../src/parseData";
import { DataError } from "../src/errors";
import * as inferType from "../src/utils/inferType";
import { GenericObject } from "../src/GenericObject";

const expectParseDataToReturn = (
  inputProps: parseDataType,
  expectedOutput: GenericObject
) => {
  expect(parseData(inputProps)).toEqual(expectedOutput);
};

describe("parseData", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("returns an empty object with just a blank positional array", () => {
    expect(parseData({ posArgs: [] })).toEqual({});
  });

  it("parses data that is paired with keys into the return payload", () => {
    expectParseDataToReturn({ posArgs: ["abc=def"] }, { abc: "def" });
    expectParseDataToReturn(
      { posArgs: ["first=arg", "second=another"] },
      { first: "arg", second: "another" }
    );
    expectParseDataToReturn({ posArgs: ["blank="] }, { blank: "" });
  });

  it("keeps extra equals signs in the value string", () => {
    expectParseDataToReturn(
      { posArgs: ["equation=1+2=3"] },
      { equation: "1+2=3" }
    );
    expectParseDataToReturn({ posArgs: ["eqSep====="] }, { eqSep: "====" });
  });

  it("throws error with extra data and no leniency", () => {
    expect(() => parseData({ posArgs: ["keyless"] })).toThrowError(DataError);
    expect(() =>
      parseData({ posArgs: ["these", "data", "have", "no", "keys"] })
    ).toThrowError(DataError);
    expect(() =>
      parseData({ required: ["key1"], posArgs: ["hasKey", "noKey"] })
    ).toThrowError(DataError);
  });

  it("saves extra data when lenient", () => {
    expectParseDataToReturn(
      { lenient: true, posArgs: ["keyless"] },
      { extraData: "keyless" }
    );
    expectParseDataToReturn(
      { lenient: true, posArgs: [3, "[1, 2, three]", "{a: bcd}"] },
      { extraData: [3, [1, 2, "three"], { a: "bcd" }] }
    );
    expectParseDataToReturn(
      {
        lenient: true,
        posArgs: ["extra=Data", "can", "be=interspersed", "with", "keyed=data"],
      },
      {
        extra: "Data",
        be: "interspersed",
        keyed: "data",
        extraData: ["can", "with"],
      }
    );
  });

  it("assigns data to required keys", () => {
    expectParseDataToReturn(
      { required: "abc", posArgs: ["value"] },
      { abc: "value" }
    );
    expectParseDataToReturn(
      { required: ["a", "b"], posArgs: ["first", "second"] },
      { a: "first", b: "second" }
    );
    expectParseDataToReturn(
      {
        required: ["a", "b"],
        posArgs: ["first", "second", "third"],
        lenient: true,
      },
      { a: "first", b: "second", extraData: "third" }
    );
  });

  it("throws if not enough data is given for all required keys", () => {
    expect(() => parseData({ required: "a", posArgs: [] })).toThrowError(
      DataError
    );
    expect(() =>
      parseData({ required: ["a", "b"], posArgs: ["onlyOne"] })
    ).toThrowError(DataError);
    expect(() =>
      parseData({
        required: ["a", "b"],
        posArgs: ["lenientDoesNotHelp"],
        lenient: true,
      })
    ).toThrowError(DataError);
  });

  it("handles optional extra keys", () => {
    expectParseDataToReturn(
      { optional: "abc", posArgs: ["cde"] },
      { abc: "cde" }
    );
    expectParseDataToReturn({ optional: "optional", posArgs: [] }, {});
    expectParseDataToReturn(
      { optional: "withDefault=3", posArgs: [] },
      { withDefault: 3 }
    );
    expectParseDataToReturn(
      { optional: "withBlankDefault=", posArgs: [] },
      { withBlankDefault: "" }
    );
    expectParseDataToReturn(
      { optional: "withDefault=3", posArgs: ["replacement"] },
      { withDefault: "replacement" }
    );
  });

  it("replaces default value on optional keys if explicitly specified", () => {
    expectParseDataToReturn(
      { optional: "abc", posArgs: ["abc=cde", "ghi"], lenient: true },
      { abc: "cde", extraData: "ghi" }
    );
    expectParseDataToReturn(
      {
        optional: "abc=123",
        posArgs: ["replacesAbc", "abc=replacesAgain"],
      },
      { abc: "replacesAgain" }
    );
    expectParseDataToReturn(
      {
        optional: ["first", "second=42"],
        posArgs: ["first=54", "[3]"],
      },
      { first: 54, second: [3] }
    );
    expectParseDataToReturn(
      {
        optional: ["first=123", "second=42"],
        posArgs: ["second=54"],
      },
      { first: 123, second: 54 }
    );
  });

  it("calls inferType for all kinds of data entry", () => {
    const testCases: [parseDataType, number][] = [
      [{ posArgs: ["withKey=data"] }, 1],
      [{ posArgs: ["extraArg"], lenient: true }, 1],
      [{ required: ["keyIs"], posArgs: ["given"] }, 1],
      [{ posArgs: [] }, 0],
      [{ optional: "has=defaultValue", posArgs: [] }, 1],
      [{ optional: ["onlyFinalData=goesThrough"], posArgs: ["inferType"] }, 1],
      [{ field: "[1,2,3]", posArgs: [] }, 1],
      [{ comment: "comment", posArgs: [] }, 1],
      [{ comment: "comment1", posArgs: ["comment=[123]"] }, 2],
    ];

    testCases.forEach((testCase) => {
      const parseDataArgs: parseDataType = testCase[0];
      const inferTypeCalls = testCase[1];

      const spy = jest.spyOn(inferType, "default");
      parseData(parseDataArgs);

      expect(spy).toHaveBeenCalledTimes(inferTypeCalls);
      spy.mockClear();
    });
  });

  it("uses the field prop to populate the field key, overwriting manual spec", () => {
    expectParseDataToReturn(
      { field: "fromProps", posArgs: [] },
      { field: "fromProps" }
    );
    expectParseDataToReturn(
      { posArgs: ["field=fromExtra"] },
      { field: "fromExtra" }
    );
    expectParseDataToReturn(
      { field: "fromProps", posArgs: ["field=fromExtra"] },
      { field: "fromProps" }
    );
  });

  it("only uses the last field specified", () => {
    expectParseDataToReturn(
      { field: ["fromProps1", "fromProps2"], posArgs: ["field=fromExtra"] },
      { field: "fromProps2" }
    );
  });

  it("saves comments from args", () => {
    expectParseDataToReturn(
      { comment: "this is a comment.", posArgs: [] },
      { comment: "this is a comment." }
    );
    expectParseDataToReturn(
      {
        comment: ["this is a comment.", "more comments get added in array"],
        posArgs: [],
      },
      { comment: ["this is a comment.", "more comments get added in array"] }
    );
  });

  it("concats data comments to arg comments", () => {
    expectParseDataToReturn(
      { comment: "argComment", posArgs: ["comment=dataComment"] },
      { comment: ["dataComment", "argComment"] }
    );
    expectParseDataToReturn(
      {
        comment: ["argComment1", "argComment2"],
        posArgs: ["comment=dataComment"],
      },
      { comment: ["dataComment", "argComment1", "argComment2"] }
    );
  });

  it("uses the remainder key for any extra data", () => {
    expectParseDataToReturn(
      { remainder: "rem", posArgs: ["oneArgHasNoArray"] },
      { rem: "oneArgHasNoArray" }
    );
    expectParseDataToReturn(
      { remainder: "rem", posArgs: ["abc", "otherKey=def", "hij"] },
      { rem: ["abc", "hij"], otherKey: "def" }
    );
    expectParseDataToReturn(
      { required: ["keyA"], remainder: "rem", posArgs: ["abc", "def", "hij"] },
      { keyA: "abc", rem: ["def", "hij"] }
    );
  });

  it("appends to the remainder key if previously specified", () => {
    expectParseDataToReturn(
      { remainder: "rem", posArgs: ["rem=abc", "def"] },
      { rem: ["abc", "def"] }
    );
    expectParseDataToReturn(
      {
        remainder: "rem",
        required: ["rem", "other"],
        posArgs: ["first", "second", "third"],
      },
      { other: "second", rem: ["first", "third"] }
    );
    expectParseDataToReturn(
      {
        remainder: "rem",
        optional: ["rem", "other"],
        posArgs: ["first", "second", "third"],
      },
      { other: "second", rem: ["first", "third"] }
    );
  });

  it("can interpret remainder data as a single string with the stringRemainder option", () => {
    expectParseDataToReturn(
      {
        stringRemainder: true,
        remainder: "rem",
        optional: "firstKey",
        posArgs: [
          "firstArg",
          "Additional",
          "arguments",
          "get",
          "made",
          "into",
          "a",
          "single",
          "string.",
        ],
      },
      {
        firstKey: "firstArg",
        rem: "Additional arguments get made into a single string.",
      }
    );
  });

  it("appends remaining postArgs as a single string to an array of the remainder key if previously specified", () => {
    expectParseDataToReturn(
      {
        stringRemainder: true,
        remainder: "rem",
        posArgs: ["rem=elementInArray", "Rest", "as", "string"],
      },
      { rem: ["elementInArray", "Rest as string"] }
    );
  });

  it("fills all required keys before the optional keys", () => {
    expectParseDataToReturn(
      {
        required: ["req1", "req2"],
        optional: ["opt1", "opt2=defaultValue"],
        posArgs: ["value1", "value2", "value3", "value4"],
      },
      { req1: "value1", req2: "value2", opt1: "value3", opt2: "value4" }
    );
  });
});
