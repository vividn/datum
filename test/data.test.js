const dataImports = require("../src/data");
const {
  parseData,
  inferType,
  DataError,
  KeysError,
  __RewireAPI__,
  splitFirstEquals,
} = dataImports;

const expectFromCases = (testCases) => {
  testCases.forEach((testCase) => {
    const params = testCase[0];
    const expectedOutput = testCase[1];

    expect(parseData(params), `${JSON.stringify(params)}`).toEqual(
      expectedOutput
    );
  });
};

describe("parseData", () => {
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

  it("automatically converts data into numbers", () => {
    const testCases = [
      [{ posArgs: ["abc=3"] }, { abc: 3 }],
      [{ posArgs: ["def=-7"] }, { def: -7 }],
      [{ posArgs: ["ghi=79.31"] }, { ghi: 79.31 }],
    ];
    expectFromCases(testCases);
  });

  it("converts array looking data", () => {
    const testCases = [
      [{ posArgs: ["abc=[3, 4, 5]"] }, { abc: [3, 4, 5] }],
      [{ posArgs: ["def=[a, b ,c]"] }, { def: ["a", "b", "c"] }],
      [{ posArgs: ["empty=[]"] }, { empty: [] }],
      [
        { posArgs: ["nested=[a, 3, [mixed, [2], nested]]"] },
        { nested: ["a", 3, ["mixed", [2], "nested"]] },
      ],
    ];
    expectFromCases(testCases);
  });

  it("convert JSON looking data", () => {
    const testCases = [
      [{ posArgs: ["empty={}"] }, { empty: {} }],
      [{ posArgs: ["two41={a: bcd, d: 3}"] }, { two41: { d: 3, a: "bcd" } }],
      [
        {
          posArgs: [
            "turtles={all: {the: {way: down}}}",
            "flat={earth: [or, 1, turtle, shell]}",
          ],
        },
        {
          turtles: { all: { the: { way: "down" } } },
          flat: { earth: ["or", 1, "turtle", "shell"] },
        },
      ],
    ];
    expectFromCases(testCases);
  });

  it("parses null values", () => {
    const testCases = [
      [{ posArgs: ["abc=null"] }, { abc: null }],
      [{ posArgs: ["abc=NULL"] }, { abc: null }],
    ];
    expectFromCases(testCases);
  });

  it("parses NaN values", () => {
    const testCases = [
      [{ posArgs: ["abc=nan"] }, { abc: Number.NaN }],
      [{ posArgs: ["abc=NaN"] }, { abc: Number.NaN }],
      [{ posArgs: ["abc=NAN"] }, { abc: Number.NaN }],
    ];
    expectFromCases(testCases);
  });

  it("parses weird looking things as strings", () => {
    const testCases = [
      [
        { posArgs: ["abc={what: even is this)) ]}"] },
        { abc: "{what: even is this)) ]}" },
      ],
      [{ posArgs: ["abc=(1,2,33)"] }, { abc: "(1,2,33)" }],
      [{ posArgs: ["abc=NAN/null"] }, { abc: "NAN/null" }],
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
    expect(true).toBe(false)
  })

  it("throws if not enough data is given for all required keys", () => {
    expect(true).toBe(false)
  })

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
          posArgs: ["cde", "abc=fromPosArgs"],
          lenient: true,
        },
        { abc: "fromPosArgs", extraData: ["cde"] },
      ],
      [
        {
          extraKeys: ["first=", "second=42"],
          posArgs: ["first=54", "[3]"],
          lenient: true,
        },
        { first: 54, second: [3] },
      ],
    ];
    expectFromCases(testCases);
  });

  it("calls inferType for all kinds of data entry", () => {
    const testCases = [
      [{ posArgs: ["extraArg"], lenient: true }, 1],
      [{ posArgs: ["withKey=data"] }, 1],
      [{ extraKeys: ["keyIs"], posArgs: ["given"] }, 1],
      [{ posArgs: [] }, 0],
      [{ extraKeys: ["onlyFinalData=goesThrough"], posArgs: ["inferType"] }, 1],
    ];

    testCases.forEach((testCase) => {
      const parseDataArgs = testCase[0];
      const inferTypeCalls = testCase[1];

      const mockedInferType = jest.fn(() => "returnData");
      dataImports.__Rewire__({ inferType: mockedInferType });
      parseData(parseDataArgs);
      expect(
        mockedInferType,
        `${JSON.stringify(testCase)}`
      ).toHaveBeenCalledTimes(inferTypeCalls);
    });
  });
});

describe("inferType", () => {
  it("leaves numbers as numbers", () => {
    expect(inferType(3)).toBe(3);
    expect(inferType(-45.5)).toBe(-45.5);
  });

  it("converts strings that are number to numbers", () => {
    expect(inferType("3")).toBe(3);
    expect(inferType("-45.5")).toBe(-45.5);
  });

  it("handles special number strings", () => {
    expect(inferType("nan")).toBe(Number.NaN);
    expect(inferType("NaN")).toBe(Number.NaN);
    expect(inferType("NAN")).toBe(Number.NaN);

    expect(inferType("null")).toBe(null);
    expect(inferType("NULL")).toBe(null);

    expect(inferType("inf")).toBe(Number.POSITIVE_INFINITY);
    expect(inferType("infinity")).toBe(Number.POSITIVE_INFINITY);
    expect(inferType("-inf")).toBe(Number.NEGATIVE_INFINITY);
    expect(inferType("-infinity")).toBe(Number.NEGATIVE_INFINITY);
  });

  it("converts array looking data", () => {
    expect(inferType([3, 4, 5])).toEqual([3, 4, 5]);
    expect(inferType("[a, b ,c]")).toEqual(["a", "b", "c"]);
    expect(inferType("[]")).toEqual([]);
    expect(inferType("[a, 3, [mixed, [2], nested]]")).toEqual([
      "a",
      3,
      ["mixed", [2], "nested"],
    ]);
  });

  it("converts JSON looking data", () => {
    expect(inferType("{}")).toEqual({});
    expect(inferType("{a: bcd, d: 3}")).toEqual({ d: 3, a: "bcd" });
    expect(inferType("{turtles: {all: {the: {way: down}}}}")).toEqual({
      turtles: { all: { the: { way: "down" } } },
    });
    expect(inferType("{flat: {earth: [or, 1, turtle, shell]}}")).toEqual({
      flat: { earth: ["or", 1, "turtle", "shell"] },
    });
  });

  it("parses weird looking things as strings", () => {
    expect(inferType("{what: even is this)) ]}")).toEqual(
      "{what: even is this)) ]}"
    );
    expect(inferType("(1,2,3)")).toEqual("(1,2,3)");
    expect(inferType("NAN/null")).toEqual("NAN/null");
  });
});

describe("splitFirstEquals", () => {
  it("returns [str, undefined] if there are no equals signs", () => {
    expect(splitFirstEquals("")).toStrictEqual(["", undefined]);
    expect(splitFirstEquals("a")).toStrictEqual(["a", undefined]);
    expect(splitFirstEquals("a,bsdflkj3")).toStrictEqual(["a,bsdflkj3", undefined]);
  });

  it("returns key value pair when there is one equals", () => {
    expect(splitFirstEquals("a=b")).toStrictEqual(["a", "b"]);
    expect(splitFirstEquals("a=")).toStrictEqual(["a", ""]);
    expect(splitFirstEquals("abc=def")).toStrictEqual(["abc", "def"]);
  });

  it("puts any extra equals signs into the value of the pair", () => {
    expect(splitFirstEquals("a=b=c")).toStrictEqual(["a", "b=c"]);
    expect(splitFirstEquals("a====")).toStrictEqual(["a", "==="]);
  });
});
