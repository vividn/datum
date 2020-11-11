const utils = require("../src/utils");
import { parseData, DataError, KeysError } from "../src/data";

const expectFromCases = (testCases) => {
  testCases.forEach((testCase) => {
    const params = testCase[0];
    const expectedOutput = testCase[1];

    expect(parseData(params), `${JSON.stringify(params)}`).toEqual(
      expectedOutput
    );
  });
};

const expectParseDataToReturn = (inputProps, expectedOutput) => {
  expect(parseData(inputProps), `${JSON.stringify(inputProps)}`).toEqual(
    expectedOutput
  );
}

describe("parseData", () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it("returns an empty object with just a blank positional array", () => {
    expect(parseData({ posArgs: [] })).toEqual({});
  });

  it("parses data that is paired with keys into the return payload", () => {
    const testCases = [
      [{ posArgs: ["abc=def"] }, { abc: "def" }],
      [
        { posArgs: ["first=arg", "second=another"] },
        { first: "arg", second: "another" },
      ],
    ];
    expectFromCases(testCases);
  });

  it("keeps extra equals signs in the value string", () => {
    const testCases = [
      [{ posArgs: ["equation=1+2=3"] }, { equation: "1+2=3" }],
      [{ posArgs: ["eqSep====="] }, { eqSep: "====" }],
    ];
    expectFromCases(testCases);
  });

  it("throws error with extra data and no leniency", () => {
    expect(() => parseData({ posArgs: ["keyless"] })).toThrowError(DataError);
    expect(() =>
      parseData({ posArgs: ["these", "data", "have", "no", "keys"] })
    ).toThrowError(DataError);
    expect(() =>
      parseData({ extraKeys: ["key1"], posArgs: ["hasKey", "noKey"] })
    ).toThrowError(DataError);
  });

  it("saves extra data when lenient", () => {
    const testCases = [
      [{ lenient: true, posArgs: ["keyless"] }, { extraData: ["keyless"] }],
      [
        { lenient: true, posArgs: [3, "[1, 2, three]", "{a: bcd}"] },
        { extraData: [3, [1, 2, "three"], { a: "bcd" }] },
      ],
      [
        {
          lenient: true,
          posArgs: [
            "extra=Data",
            "can",
            "be=interspersed",
            "with",
            "keyed=data",
          ],
        },
        {
          extra: "Data",
          be: "interspersed",
          keyed: "data",
          extraData: ["can", "with"],
        },
      ],
    ];
    expectFromCases(testCases);
  });

  it("assigns data to required keys", () => {
    const testCases = [
      [{ extraKeys: "abc", posArgs: ["value"] }, { abc: "value" }],
      [
        { extraKeys: ["a", "b"], posArgs: ["first", "second"] },
        { a: "first", b: "second" },
      ],
      [
        {
          extraKeys: ["a", "b"],
          posArgs: ["first", "second", "third"],
          lenient: true,
        },
        { a: "first", b: "second", extraData: ["third"] },
      ],
    ];
    expectFromCases(testCases);
  });

  it("throws if not enough data is given for all required keys", () => {
    expect(() => parseData({ extraKeys: "a", posArgs: [] })).toThrowError(
      DataError
    );
    expect(() =>
      parseData({ extraKeys: ["a", "b"], posArgs: ["onlyOne"] })
    ).toThrowError(DataError);
    expect(() =>
      parseData({
        extraKeys: ["a", "b"],
        posArgs: ["lenientDoesNotHelp"],
        lenient: true,
      })
    ).toThrowError(DataError);
  });

  it("handles optional extra keys", () => {
    const testCases = [
      [{ extraKeys: "abc=", posArgs: ["cde"] }, { abc: "cde" }],
      [{ extraKeys: "optional=", posArgs: [] }, {}],
      [{ extraKeys: "optWithDefault=3", posArgs: [] }, { optWithDefault: 3 }],
      [
        { extraKeys: "optWithDefault=3", posArgs: ["replacement"] },
        { optWithDefault: "replacement" },
      ],
    ];
    expectFromCases(testCases);
  });

  it("must have all required keys before optional keys", () => {
    expect(() =>
      parseData({
        extraKeys: ["req1", "opt1=", "req2"],
        posArgs: ["withTwo", "arguments"],
      })
    ).toThrowError(KeysError);
    expect(() =>
      parseData({
        extraKeys: ["req1", "opt1=", "req2"],
        posArgs: ["also", "with", "three"],
      })
    ).toThrowError(KeysError);
  });

  it("replaces extra keys if explicitly specified", () => {
    const testCases = [
      [
        { extraKeys: "abc=", posArgs: ["abc=cde", "ghi"], lenient: true },
        { abc: "cde", extraData: ["ghi"] },
      ],
      [
        {
          extraKeys: "abc=123",
          posArgs: ["replacesAbc", "abc=replacesAgain"],
          lenient: true,
        },
        { abc: "replacesAgain" },
      ],
      [
        {
          extraKeys: ["first=", "second=42"],
          posArgs: ["first=54", "[3]"],
          lenient: true,
        },
        { first: 54, second: [3] },
      ],
      [
        {
          extraKeys: ["first=123", "second=42"],
          posArgs: ["second=54"],
        },
        { first: 123, second: 54 },
      ],
    ];
    expectFromCases(testCases);
  });

  it("calls inferType for all kinds of data entry", () => {
    const testCases = [
      [{ posArgs: ["withKey=data"] }, 1],
      [{ posArgs: ["extraArg"], lenient: true }, 1],
      [{ extraKeys: ["keyIs"], posArgs: ["given"] }, 1],
      [{ posArgs: [] }, 0],
      [{ extraKeys: ["onlyFinalData=goesThrough"], posArgs: ["inferType"] }, 1],
      [{ field: "[1,2,3]", posArgs: []}, 1],
      [{ comment: "comment", posArgs: []}, 1],
      [{ comment: "comment1", posArgs: ["comment=[123]"]}, 2]
    ];

    testCases.forEach((testCase) => {
      const parseDataArgs = testCase[0];
      const inferTypeCalls = testCase[1];

      jest.clearAllMocks();
      const mockedInferType = jest.spyOn(utils, "inferType");
      parseData(parseDataArgs)

      expect(
        mockedInferType,
        `${JSON.stringify(testCase)}`
      ).toHaveBeenCalledTimes(inferTypeCalls);
    });
  });

  it("uses the field prop to populate the field key, overwriting manual spec", () => {
    expectParseDataToReturn({ field: "fromProps", posArgs: [] }, { field: "fromProps"})
    expectParseDataToReturn({ posArgs: ["field=fromExtra"] }, { field: "fromExtra"})
    expectParseDataToReturn({ field: "fromProps", posArgs: ["field=fromExtra"] }, { field: "fromProps"})
  })

  it("only uses the last field specified", () => {
    expectParseDataToReturn( {field: ["fromProps1", "fromProps2"], posArgs: ["field=fromExtra"]}, { field: "fromProps2" })
  })

  it("saves comments from args", () => {
    expectParseDataToReturn( { comment: "this is a comment.", posArgs: [] }, { comment: "this is a comment." })
    expectParseDataToReturn( { comment: ["this is a comment.", "more comments get added in array"], posArgs: [] }, { comment: ["this is a comment.", "more comments get added in array"] })   
  })

  it("concats data comments to arg comments", () => {
    expectParseDataToReturn( { comment: "argComment", posArgs: ["comment=dataComment"]}, { comment: ["dataComment", "argComment"]})
    expectParseDataToReturn( { comment: ["argComment1", "argComment2"], posArgs: ["comment=dataComment"]}, { comment: ["dataComment", "argComment1", "argComment2"]})
  })

});
