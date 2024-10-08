import { DataArgs, handleDataArgs } from "../dataArgs";
import { ExtraDataError, MissingRequiredKeyError } from "../../errors";
import * as inferType from "../../utils/inferType";
import spyOn = jest.spyOn;
import * as changeDatumCommandModule from "../../utils/changeDatumCommand";
import { jClone } from "../../utils/jClone";
import { GenericObject } from "../../utils/utilityTypes";

const expectParseDataToReturn = (
  inputProps: DataArgs,
  expectedOutput: GenericObject,
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
      { first: "arg", second: "another" },
    );
    expectParseDataToReturn({ data: ["blank="] }, {});
  });

  it("keeps extra equals signs in the value string", () => {
    expectParseDataToReturn(
      { data: ["equation=1+2=3"] },
      { equation: "1+2=3" },
    );
    expectParseDataToReturn({ data: ["eqSep====="] }, { eqSep: "====" });
  });

  it("throws error with extra data and no leniency", () => {
    expect(() => handleDataArgs({ data: ["keyless"] })).toThrow(ExtraDataError);
    expect(() =>
      handleDataArgs({ data: ["these", "data", "have", "no", "keys"] }),
    ).toThrow(ExtraDataError);
    expect(() =>
      handleDataArgs({ keys: ["key1"], data: ["hasKey", "noKey"] }),
    ).toThrow(ExtraDataError);
  });

  it("saves extra data when lenient", () => {
    expectParseDataToReturn(
      { lenient: true, data: ["keyless"] },
      { extraData: "keyless" },
    );
    expectParseDataToReturn(
      { lenient: true, data: [3, "[1, 2, three]", "{a: bcd}"] },
      { extraData: [3, [1, 2, "three"], { a: "bcd" }] },
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
      },
    );
  });

  it("assigns data to required keys", () => {
    expectParseDataToReturn(
      { keys: ["abc"], data: ["value"] },
      { abc: "value" },
    );
    expectParseDataToReturn(
      { keys: ["a", "b"], data: ["first", "second"] },
      { a: "first", b: "second" },
    );
    expectParseDataToReturn(
      {
        keys: ["a", "b"],
        data: ["first", "second", "third"],
        lenient: true,
      },
      { a: "first", b: "second", extraData: "third" },
    );
  });

  it("throws if not enough data is given for all required keys", () => {
    expect(() => handleDataArgs({ keys: ["a"], data: [] })).toThrow(
      MissingRequiredKeyError,
    );
    expect(() =>
      handleDataArgs({ keys: ["a", "b"], data: ["onlyOne"] }),
    ).toThrow(MissingRequiredKeyError);
    expect(() =>
      handleDataArgs({
        keys: ["a", "b"],
        data: ["lenientDoesNotHelp"],
        lenient: true,
      }),
    ).toThrow(MissingRequiredKeyError);
  });

  test("required keys can be specified manually without error", () => {
    expectParseDataToReturn(
      { keys: ["abc"], data: ["abc=def"] },
      { abc: "def" },
    );
  });

  test("manually specified required keys overwrite already specified auto values", () => {
    expectParseDataToReturn(
      { keys: ["abc"], data: ["def", "abc=ghi"] },
      { abc: "ghi" },
    );
  });

  test("future keyless data skips over manually specified required keys", () => {
    expectParseDataToReturn(
      { keys: ["abc", "def", "ghi"], data: ["def=123", "xyz", "foobar"] },
      { abc: "xyz", def: 123, ghi: "foobar" },
    );
  });

  it("handles optional extra keys", () => {
    expectParseDataToReturn({ keys: ["abc="], data: ["cde"] }, { abc: "cde" });
    expectParseDataToReturn({ keys: ["optional="], data: [] }, {});
    expectParseDataToReturn(
      { keys: ["withDefault=3"], data: [] },
      { withDefault: 3 },
    );
    expectParseDataToReturn(
      { keys: ["withBlankDefault="], data: [] },
      { withBlankDefault: undefined },
    );
    expectParseDataToReturn(
      { keys: ["withEmptyStringDefault=''"], data: [] },
      { withEmptyStringDefault: "" },
    );
    expectParseDataToReturn(
      { keys: ["withDefault=3"], data: ["replacement"] },
      { withDefault: "replacement" },
    );
  });

  it("replaces default value on optional keys if explicitly specified", () => {
    expectParseDataToReturn(
      { keys: ["abc="], data: ["abc=cde", "ghi"], lenient: true },
      { abc: "cde", extraData: "ghi" },
    );
    expectParseDataToReturn(
      {
        keys: ["abc=123"],
        data: ["replacesAbc", "abc=replacesAgain"],
      },
      { abc: "replacesAgain" },
    );
    expectParseDataToReturn(
      {
        keys: ["first=", "second=42"],
        data: ["first=54", "[3]"],
      },
      { first: 54, second: [3] },
    );
    expectParseDataToReturn(
      {
        keys: ["first=123", "second=42"],
        data: ["second=54"],
      },
      { first: 123, second: 54 },
    );
  });

  it.each([
    [{ data: ["withKey=data"] }, 1],
    [{ data: ["extraArg"], lenient: true }, 1],
    [{ keys: ["keyIs"], data: ["given"] }, 1],
    [{ data: [] }, 0],
    [{ keys: ["has=defaultValue"], data: [] }, 1],
    [{ keys: ["onlyFinalData=goesThrough"], data: ["inferType"] }, 1],
    [{ comment: "comment", data: [] }, 1],
    [{ comment: "comment1", data: ["comment=[123]"] }, 2],
  ])(
    "When called with %s, inferType is called %i times",
    (parseDataArgs: DataArgs, inferTypeCalls: number) => {
      const spy = jest.spyOn(inferType, "inferType");
      handleDataArgs(parseDataArgs);

      expect(spy).toHaveBeenCalledTimes(inferTypeCalls);
    },
  );

  it("saves comments from args", () => {
    expectParseDataToReturn(
      { comment: "this is a comment.", data: [] },
      { comment: "this is a comment." },
    );
    expectParseDataToReturn(
      {
        comment: ["this is a comment.", "more comments get added in array"],
        data: [],
      },
      { comment: ["this is a comment.", "more comments get added in array"] },
    );
  });

  it("concats data comments to arg comments", () => {
    expectParseDataToReturn(
      { comment: "argComment", data: ["comment=dataComment"] },
      { comment: ["dataComment", "argComment"] },
    );
    expectParseDataToReturn(
      {
        comment: ["argComment1", "argComment2"],
        data: ["comment=dataComment"],
      },
      { comment: ["dataComment", "argComment1", "argComment2"] },
    );
  });

  it("uses the remainder key for any extra data", () => {
    expectParseDataToReturn(
      { remainder: "rem", data: ["oneArgHasNoArray"] },
      { rem: "oneArgHasNoArray" },
    );
    expectParseDataToReturn(
      { remainder: "rem", data: ["abc", "otherKey=def", "hij"] },
      { rem: ["abc", "hij"], otherKey: "def" },
    );
    expectParseDataToReturn(
      { keys: ["keyA"], remainder: "rem", data: ["abc", "def", "hij"] },
      { keyA: "abc", rem: ["def", "hij"] },
    );
  });

  it("appends to the remainder key if previously specified", () => {
    expectParseDataToReturn(
      { remainder: "rem", data: ["rem=abc", "def"] },
      { rem: ["abc", "def"] },
    );
    expectParseDataToReturn(
      {
        remainder: "rem",
        keys: ["rem", "other"],
        data: ["first", "second", "third"],
      },
      { other: "second", rem: ["first", "third"] },
    );
    expectParseDataToReturn(
      {
        remainder: "rem",
        keys: ["rem=", "other="],
        data: ["first", "second", "third"],
      },
      { other: "second", rem: ["first", "third"] },
    );
  });

  it("can interpret remainder data as a single string with the stringRemainder option", () => {
    expectParseDataToReturn(
      {
        stringRemainder: true,
        remainder: "rem",
        keys: ["firstKey="],
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
      },
    );
  });

  it("appends remaining postArgs as a single string to an array of the remainder key if previously specified", () => {
    expectParseDataToReturn(
      {
        stringRemainder: true,
        remainder: "rem",
        data: ["rem=elementInArray", "Rest", "as", "string"],
      },
      { rem: ["elementInArray", "Rest as string"] },
    );
  });

  it("can easily do a comment string remainder", () => {
    expectParseDataToReturn(
      {
        commentRemainder: true,
        keys: ["optional1="],
        data: [
          "the",
          "non",
          "filled",
          "keys",
          "will",
          "key=value",
          "be",
          "a",
          "comment.",
        ],
      },
      {
        optional1: "the",
        key: "value",
        comment: "non filled keys will be a comment.",
      },
    );
  });

  it("appends the comment remainder to other comments as an array", () => {
    expectParseDataToReturn(
      {
        commentRemainder: true,
        comment: "explicit comment",
        data: ["comment", "remainder", "string"],
      },
      {
        comment: ["explicit comment", "comment remainder string"],
      },
    );
    expectParseDataToReturn(
      {
        commentRemainder: true,
        comment: ["comment1", "comment2"],
        data: ["comment", "remainder", "string"],
      },
      {
        comment: ["comment1", "comment2", "comment remainder string"],
      },
    );
  });

  it("can intersperse required and optional keys", () => {
    expectParseDataToReturn(
      {
        keys: ["req1", "opt1=", "req2", "opt2=defaultValue"],
        data: ["value1", "value2", "value3", "value4"],
      },
      { req1: "value1", opt1: "value2", req2: "value3", opt2: "value4" },
    );
  });

  it("allows a required key to be given a default value later via an optional key", () => {
    expectParseDataToReturn(
      {
        keys: [
          "req1",
          "withDefaultFromOptional",
          "opt1=",
          "withDefaultFromOptional=noError",
        ],
        data: ["onlyOneValue"],
      },
      { req1: "onlyOneValue", withDefaultFromOptional: "noError" },
    );
  });

  it("maintains the first seen ordering of keys, even when one is specied later, e.g. to provide a default", () => {
    expectParseDataToReturn(
      {
        keys: [
          "req1",
          "withDefaultFromOptional",
          "opt1=",
          "withDefaultFromOptional=noError",
        ],
        data: ["one", "two", "three"],
      },
      { req1: "one", withDefaultFromOptional: "two", opt1: "three" },
    );
    expect(() =>
      handleDataArgs({
        keys: [
          "req1",
          "withDefaultFromOptional",
          "opt1=",
          "withDefaultFromOptional=noError",
        ],
        data: ["one", "two", "three", "four"],
      }),
    ).toThrow(ExtraDataError);
  });

  it("infers type from key for key=value data entry", () => {
    expectParseDataToReturn(
      { data: ["taskDuration=3hrs"] },
      {
        taskDuration: "PT3H",
      },
    );
  });

  it("infers type from key for required keys", () => {
    expectParseDataToReturn(
      { keys: ["dueDate"], data: ["Dec 31, 2021"] },
      {
        dueDate: "2021-12-31",
      },
    );
  });

  it("infers type from key for optional keys", () => {
    expectParseDataToReturn(
      { keys: ["startDate="], data: ["June 13, 2020"] },
      {
        startDate: "2020-06-13",
      },
    );
  });

  it("infers type from key for default values", () => {
    expectParseDataToReturn(
      { keys: ["duration=5"] },
      {
        duration: "PT5M",
      },
    );
  });

  it("does not modify args directly", () => {
    const args: DataArgs = {
      keys: ["req1", "req2", "opt1=", "opt2=defaultValue"],
      remainder: "remainderKey",
      baseData: '{ abc: "def" }',
      comment: "This is a comment.",
      data: ["value1", "value2", "value3", "value4", "value5", "value6"],
    };
    const argsClone = jClone(args);
    const parsedData1 = handleDataArgs(args);
    expect(parsedData1).toEqual({
      req1: "value1",
      req2: "value2",
      opt1: "value3",
      opt2: "value4",
      remainderKey: ["value5", "value6"],
      abc: "def",
      comment: "This is a comment.",
    });
    expect(args).toEqual(argsClone);
  });

  it("uses the default value for an optional key if '.' is given as the value", () => {
    expectParseDataToReturn(
      {
        keys: [
          "req1",
          "opt1=dotDefault",
          "opt2=overwrittenDefault",
          "opt3=notGivenDefault",
        ],
        data: ["oneValue", ".", "threeValue"],
      },
      {
        req1: "oneValue",
        opt1: "dotDefault",
        opt2: "threeValue",
        opt3: "notGivenDefault",
      },
    );
  });

  it("can use the default value if . is specified explicitly as the value for an optional key", () => {
    expectParseDataToReturn(
      {
        keys: [
          "req1",
          "opt1=overwrittenDefault",
          "opt2=dotDefault",
          "opt3=notGivenDefault",
        ],
        data: ["opt2=.", "twoValue", "threeValue"],
      },
      {
        req1: "twoValue",
        opt1: "threeValue",
        opt2: "dotDefault",
        opt3: "notGivenDefault",
      },
    );
    expectParseDataToReturn(
      {
        keys: [
          "req1",
          "opt1=overwritten1",
          "opt2=dotDefault",
          "opt3=overwritten2",
        ],
        data: ["opt2=.", "two", "three", "four"],
      },
      {
        req1: "two",
        opt1: "three",
        opt2: "dotDefault",
        opt3: "four",
      },
    );
  });

  it("can overwrite a previous defined value with undefined if an assignement with `key=` is used", () => {
    expectParseDataToReturn(
      {
        data: ["key=value", "key2=value2", "key="],
      },
      {
        key2: "value2",
      },
    );
  });

  it("assumes undefined if no default value is specified for an optional key, e.g. `-k key=`", () => {
    expectParseDataToReturn(
      {
        keys: ["key1=", "key2=", "key3=nonEmpty"],
        data: ["."],
      },
      {
        key3: "nonEmpty",
      },
    );
  });

  it("uses the previously assigned value if a . is given the second time as the value", () => {
    expectParseDataToReturn(
      {
        data: ["key=value", "key=."],
      },
      {
        key: "value",
      },
    );
    expectParseDataToReturn(
      {
        keys: ["req1"],
        data: ["someValue", "req1=."],
      },
      {
        req1: "someValue",
      },
    );
  });

  it("can explicitly set an optional arg to its default value by using a . before its turn in the parse chain", () => {
    expectParseDataToReturn(
      {
        keys: ["key1=value1", "key2=value2", "key3=value3"],
        data: ["key2=.", "replace1", "replace3"],
      },
      {
        key1: "replace1",
        key2: "value2",
        key3: "replace3",
      },
    );
  });

  it("processes pathlike data keys correctly", () => {
    expectParseDataToReturn(
      {
        keys: ["a.b", "a.c", "a.d.e=", "another.key=5"],
        data: ["data", "moreData", 7, "key.nested=abc"],
      },
      {
        a: {
          b: "data",
          c: "moreData",
          d: {
            e: 7,
          },
        },
        key: {
          nested: "abc",
        },
        another: {
          key: 5,
        },
      },
    );
  });

  it("overwrites top level data to nest data inside", () => {
    expectParseDataToReturn(
      {
        data: ["topKey=value", "topKey.nestedKey=abc"],
      },
      {
        topKey: {
          nestedKey: "abc",
        },
      },
    );
  });

  it("processes state as state.id", () => {
    expectParseDataToReturn(
      {
        data: ["state=active"],
      },
      { state: { id: "active" } },
    );
    expectParseDataToReturn(
      {
        keys: ["state"],
        data: ["inactive"],
      },
      {
        state: { id: "inactive" },
      },
    );
  });

  it("processes keys that start with . as part of state", () => {
    expectParseDataToReturn(
      {
        data: [".key=active"],
      },
      { state: { key: "active" } },
    );
    expectParseDataToReturn(
      {
        keys: [".title", ".author="],
        data: ["The Dispossessed", "Ursula K. Le Guin"],
      },
      {
        state: { title: "The Dispossessed", author: "Ursula K. Le Guin" },
      },
    );
  });

  it("can use both state= and the dot syntax to assemble a complex state", () => {
    expectParseDataToReturn(
      {
        keys: ["state="],
        data: ["active", ".type=common"],
      },
      { state: { id: "active", type: "common" } },
    );
  });

  it("can receive addtional fields from cmdData, that overwrite basedata but are overwritten by manually specified data", () => {
    expectParseDataToReturn(
      {
        baseData: { base: "b", baseAndCmd: "overwritten" },
        cmdData: {
          baseAndCmd: "bAndC",
          cmdAndExplicit: "overwritten",
          justCmd: "justCmd",
        },
        data: ["cmdAndExplicit=cAndE"],
      },
      {
        base: "b",
        baseAndCmd: "bAndC",
        cmdAndExplicit: "cAndE",
        justCmd: "justCmd",
      },
    );
  });

  it("handles deep paths for all modes of specifying a key", () => {
    const data = handleDataArgs({
      keys: ["deep.required", "deep.optional=", ".stateArg.withDefault=3"],
      data: ["value1", "direct.from=data", "value3", ".data.stateArg=4"],
    });
    expect(data).toMatchObject({
      deep: {
        required: "value1",
        optional: "value3",
      },
      direct: { from: "data" },
      state: {
        stateArg: { withDefault: 3 },
        data: { stateArg: 4 },
      },
    });
  });

  it("calls changeDatumCommand if special key dur is a special command", () => {
    const changeDatumCommandSpy = spyOn(
      changeDatumCommandModule,
      "changeDatumCommand",
    );
    const data = handleDataArgs({
      keys: ["opt1=", "dur="],
      data: ["optValue1", "start", 3],
    });
    expect(changeDatumCommandSpy).toHaveBeenCalledWith(
      expect.objectContaining({ opt1: "optValue1" }),
      "start",
      expect.objectContaining({ keys: [] }),
    );
    expect(data).toMatchObject({
      opt1: "optValue1",
      state: { id: true },
      dur: "PT3M",
    });
  });

  it("calls changeDatumCommand if an remaining data key is a special command", () => {
    const changeDatumCommandSpy = spyOn(
      changeDatumCommandModule,
      "changeDatumCommand",
    );
    const data = handleDataArgs({
      keys: ["opt1="],
      data: ["optValue1", "end", 3],
    });
    expect(changeDatumCommandSpy).toHaveBeenCalledWith(
      expect.objectContaining({ opt1: "optValue1" }),
      "end",
      expect.objectContaining({ keys: expect.arrayContaining([]) }),
    );
    expect(data).toMatchObject({
      opt1: "optValue1",
      state: { id: false },
      dur: "PT3M",
    });
  });

  it("does not switch to a different command if there is a remainderKey to grab additional arguments", () => {
    const changeDatumCommandSpy = spyOn(
      changeDatumCommandModule,
      "changeDatumCommand",
    );
    const data = handleDataArgs({
      keys: ["opt1="],
      data: ["optValue1", "end", 3],
      remainder: "remainder",
    });
    expect(changeDatumCommandSpy).not.toHaveBeenCalled();
    expect(data).toMatchObject({
      opt1: "optValue1",
      remainder: ["end", 3],
    });
  });

  it("does not allow . to be used for required keys", () => {
    expect(() =>
      handleDataArgs({
        keys: ["req1", "req2"],
        data: ["value1", "."],
      }),
    ).toThrow(MissingRequiredKeyError);
  });

  it("An optional key can be changed to be required", () => {
    expect(() =>
      handleDataArgs({
        keys: ["opt1=", "changedToReq=default", "changedToReq"],
        data: ["value1"],
      }),
    ).toThrow(MissingRequiredKeyError);
  });
});
