import { beforeEach, describe, expect, it, jest } from "@jest/globals";
import { parseData, ParseDataType } from "../src/parseData";
import { DataError } from "../src/errors";
import * as inferType from "../src/utils/inferType";
import { GenericObject } from "../src/GenericObject";

const expectParseDataToReturn = (
  inputProps: ParseDataType,
  expectedOutput: GenericObject
) => {
  expect(parseData(inputProps)).toEqual(expectedOutput);
};

describe("parseData", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("returns an empty object with just a blank positional array", () => {
    expect(parseData({ data: [] })).toEqual({});
  });

  it("parses data that is paired with keys into the return payload", () => {
    expectParseDataToReturn({ data: ["abc=def"] }, { abc: "def" });
    expectParseDataToReturn(
      { data: ["first=arg", "second=another"] },
      { first: "arg", second: "another" }
    );
    expectParseDataToReturn({ data: ["blank="] }, { blank: "" });
  });

  it("keeps extra equals signs in the value string", () => {
    expectParseDataToReturn(
      { data: ["equation=1+2=3"] },
      { equation: "1+2=3" }
    );
    expectParseDataToReturn({ data: ["eqSep====="] }, { eqSep: "====" });
  });

  it("throws error with extra data and no leniency", () => {
    expect(() => parseData({ data: ["keyless"] })).toThrowError(DataError);
    expect(() =>
      parseData({ data: ["these", "data", "have", "no", "keys"] })
    ).toThrowError(DataError);
    expect(() =>
      parseData({ required: ["key1"], data: ["hasKey", "noKey"] })
    ).toThrowError(DataError);
  });

  it("saves extra data when lenient", () => {
    expectParseDataToReturn(
      { lenient: true, data: ["keyless"] },
      { extraData: "keyless" }
    );
    expectParseDataToReturn(
      { lenient: true, data: [3, "[1, 2, three]", "{a: bcd}"] },
      { extraData: [3, [1, 2, "three"], { a: "bcd" }] }
    );
    expectParseDataToReturn(
      {
        lenient: true,
        data: ["extra=Data", "can", "be=interspersed", "with", "keyed=data"],
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
      { required: "abc", data: ["value"] },
      { abc: "value" }
    );
    expectParseDataToReturn(
      { required: ["a", "b"], data: ["first", "second"] },
      { a: "first", b: "second" }
    );
    expectParseDataToReturn(
      {
        required: ["a", "b"],
        data: ["first", "second", "third"],
        lenient: true,
      },
      { a: "first", b: "second", extraData: "third" }
    );
  });

  it("throws if not enough data is given for all required keys", () => {
    expect(() => parseData({ required: "a", data: [] })).toThrowError(
      DataError
    );
    expect(() =>
      parseData({ required: ["a", "b"], data: ["onlyOne"] })
    ).toThrowError(DataError);
    expect(() =>
      parseData({
        required: ["a", "b"],
        data: ["lenientDoesNotHelp"],
        lenient: true,
      })
    ).toThrowError(DataError);
  });

  it("handles optional extra keys", () => {
    expectParseDataToReturn({ optional: "abc", data: ["cde"] }, { abc: "cde" });
    expectParseDataToReturn({ optional: "optional", data: [] }, {});
    expectParseDataToReturn(
      { optional: "withDefault=3", data: [] },
      { withDefault: 3 }
    );
    expectParseDataToReturn(
      { optional: "withBlankDefault=", data: [] },
      { withBlankDefault: "" }
    );
    expectParseDataToReturn(
      { optional: "withDefault=3", data: ["replacement"] },
      { withDefault: "replacement" }
    );
  });

  it("replaces default value on optional keys if explicitly specified", () => {
    expectParseDataToReturn(
      { optional: "abc", data: ["abc=cde", "ghi"], lenient: true },
      { abc: "cde", extraData: "ghi" }
    );
    expectParseDataToReturn(
      {
        optional: "abc=123",
        data: ["replacesAbc", "abc=replacesAgain"],
      },
      { abc: "replacesAgain" }
    );
    expectParseDataToReturn(
      {
        optional: ["first", "second=42"],
        data: ["first=54", "[3]"],
      },
      { first: 54, second: [3] }
    );
    expectParseDataToReturn(
      {
        optional: ["first=123", "second=42"],
        data: ["second=54"],
      },
      { first: 123, second: 54 }
    );
  });

  it("calls inferType for all kinds of data entry", () => {
    const testCases: [ParseDataType, number][] = [
      [{ data: ["withKey=data"] }, 1],
      [{ data: ["extraArg"], lenient: true }, 1],
      [{ required: ["keyIs"], data: ["given"] }, 1],
      [{ data: [] }, 0],
      [{ optional: "has=defaultValue", data: [] }, 1],
      [{ optional: ["onlyFinalData=goesThrough"], data: ["inferType"] }, 1],
      [{ field: "[1,2,3]", data: [] }, 1],
      [{ comment: "comment", data: [] }, 1],
      [{ comment: "comment1", data: ["comment=[123]"] }, 2],
    ];

    testCases.forEach((testCase) => {
      const parseDataArgs: ParseDataType = testCase[0];
      const inferTypeCalls = testCase[1];

      const spy = jest.spyOn(inferType, "default");
      parseData(parseDataArgs);

      expect(spy).toHaveBeenCalledTimes(inferTypeCalls);
      spy.mockRestore();
    });
  });

  it("uses the field prop to populate the field key, overwriting manual spec", () => {
    expectParseDataToReturn(
      { field: "fromProps", data: [] },
      { field: "fromProps" }
    );
    expectParseDataToReturn(
      { data: ["field=fromExtra"] },
      { field: "fromExtra" }
    );
    expectParseDataToReturn(
      { field: "fromProps", data: ["field=fromExtra"] },
      { field: "fromProps" }
    );
  });

  it("saves comments from args", () => {
    expectParseDataToReturn(
      { comment: "this is a comment.", data: [] },
      { comment: "this is a comment." }
    );
    expectParseDataToReturn(
      {
        comment: ["this is a comment.", "more comments get added in array"],
        data: [],
      },
      { comment: ["this is a comment.", "more comments get added in array"] }
    );
  });

  it("concats data comments to arg comments", () => {
    expectParseDataToReturn(
      { comment: "argComment", data: ["comment=dataComment"] },
      { comment: ["dataComment", "argComment"] }
    );
    expectParseDataToReturn(
      {
        comment: ["argComment1", "argComment2"],
        data: ["comment=dataComment"],
      },
      { comment: ["dataComment", "argComment1", "argComment2"] }
    );
  });

  it("uses the remainder key for any extra data", () => {
    expectParseDataToReturn(
      { remainder: "rem", data: ["oneArgHasNoArray"] },
      { rem: "oneArgHasNoArray" }
    );
    expectParseDataToReturn(
      { remainder: "rem", data: ["abc", "otherKey=def", "hij"] },
      { rem: ["abc", "hij"], otherKey: "def" }
    );
    expectParseDataToReturn(
      { required: ["keyA"], remainder: "rem", data: ["abc", "def", "hij"] },
      { keyA: "abc", rem: ["def", "hij"] }
    );
  });

  it("appends to the remainder key if previously specified", () => {
    expectParseDataToReturn(
      { remainder: "rem", data: ["rem=abc", "def"] },
      { rem: ["abc", "def"] }
    );
    expectParseDataToReturn(
      {
        remainder: "rem",
        required: ["rem", "other"],
        data: ["first", "second", "third"],
      },
      { other: "second", rem: ["first", "third"] }
    );
    expectParseDataToReturn(
      {
        remainder: "rem",
        optional: ["rem", "other"],
        data: ["first", "second", "third"],
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
        data: [
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
        data: ["rem=elementInArray", "Rest", "as", "string"],
      },
      { rem: ["elementInArray", "Rest as string"] }
    );
  });

  it("fills all required keys before the optional keys", () => {
    expectParseDataToReturn(
      {
        required: ["req1", "req2"],
        optional: ["opt1", "opt2=defaultValue"],
        data: ["value1", "value2", "value3", "value4"],
      },
      { req1: "value1", req2: "value2", opt1: "value3", opt2: "value4" }
    );
  });
});
