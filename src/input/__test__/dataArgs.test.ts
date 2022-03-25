import { DataArgs, handleDataArgs } from "../dataArgs";
import { GenericObject } from "../../GenericObject";
import { DataError } from "../../errors";
import * as inferType from "../../utils/inferType";

const expectParseDataToReturn = (
  inputProps: DataArgs,
  expectedOutput: GenericObject
) => {
  expect(handleDataArgs(inputProps)).toEqual(expectedOutput);
};

describe("handleDataArgs", () => {
  it("returns an empty object with just a blank positional array", () => {
    expect(handleDataArgs({ data: [] })).toEqual({});
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
    expect(() => handleDataArgs({ data: ["keyless"] })).toThrowError(DataError);
    expect(() =>
      handleDataArgs({ data: ["these", "data", "have", "no", "keys"] })
    ).toThrowError(DataError);
    expect(() =>
      handleDataArgs({ required: ["key1"], data: ["hasKey", "noKey"] })
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
    expect(() => handleDataArgs({ required: "a", data: [] })).toThrowError(
      DataError
    );
    expect(() =>
      handleDataArgs({ required: ["a", "b"], data: ["onlyOne"] })
    ).toThrowError(DataError);
    expect(() =>
      handleDataArgs({
        required: ["a", "b"],
        data: ["lenientDoesNotHelp"],
        lenient: true,
      })
    ).toThrowError(DataError);
  });

  test("required keys can be specified manually without error", () => {
    expectParseDataToReturn(
      { required: ["abc"], data: ["abc=def"] },
      { abc: "def" }
    );
  });

  test("manually specified required keys overwrite already specified auto values", () => {
    expectParseDataToReturn(
      { required: ["abc"], data: ["def", "abc=ghi"] },
      { abc: "ghi" }
    );
  });

  test("future keyless data skips over manually specified required keys", () => {
    expectParseDataToReturn(
      { required: ["abc", "def", "ghi"], data: ["def=123", "xyz", "foobar"] },
      { abc: "xyz", def: 123, ghi: "foobar" }
    );
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

  it.each([
    [{ data: ["withKey=data"] }, 1],
    [{ data: ["extraArg"], lenient: true }, 1],
    [{ required: ["keyIs"], data: ["given"] }, 1],
    [{ data: [] }, 0],
    [{ optional: "has=defaultValue", data: [] }, 1],
    [{ optional: ["onlyFinalData=goesThrough"], data: ["inferType"] }, 1],
    [{ field: "[1,2,3]", data: [] }, 1],
    [{ comment: "comment", data: [] }, 1],
    [{ comment: "comment1", data: ["comment=[123]"] }, 2],
  ])(
    "When called with %s, inferType is called %i times",
    (parseDataArgs: DataArgs, inferTypeCalls: number) => {
      const spy = jest.spyOn(inferType, "default");
      handleDataArgs(parseDataArgs);

      expect(spy).toHaveBeenCalledTimes(inferTypeCalls);
    }
  );

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

  it("infers type from key for key=value data entry", () => {
    expectParseDataToReturn(
      { data: ["taskDuration=3hrs"] },
      {
        taskDuration: "PT3H",
      }
    );
  });

  it("infers type from key for required keys", () => {
    expectParseDataToReturn(
      { required: ["dueDate"], data: ["Dec 31, 2021"] },
      {
        dueDate: "2021-12-31",
      }
    );
  });

  it("infers type from key for optional keys", () => {
    expectParseDataToReturn(
      { optional: "startDate", data: ["June 13, 2020"] },
      {
        startDate: "2020-06-13",
      }
    );
  });

  it("infers type from key for default values", () => {
    expectParseDataToReturn(
      { optional: "duration=5" },
      {
        duration: "PT5M",
      }
    );
  });
});
