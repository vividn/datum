const { parseData, DataError } = require("../src/data");

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
      [{ posArgs: ["abc={what: even is this)) ]}"] }, { abc: "{what: even is this)) ]}" }],
      [{ posArgs: ["abc=(1,2,33)"] }, { abc: "(1,2,33)" }],
      [{ posArgs: ["abc=NAN/null"] }, { abc: "NAN/null" }],
    ];
    expectFromCases(testCases);
  })

  it("keeps extra equals signs in the value string", () => {
    const testCases = [
      [{ posArgs: ["equation=1+2=3"] }, { equation: "1+2=3"}],
      [{ posArgs: ["eqSep====="]}, { eqSep: "===="}]
    ]
    expectFromCases(testCases)
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

  it("replaces extra keys if explicityly specified", () => {
    const testCases = [
      [
        { extraKeys: "abc=", posArgs: ["abc=cde", "ghi"], lenient: true },
        { abc: "cde", extraData: ["ghi"] },
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
});
