const { parseData } = require("../src/data");

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
      [{ posArgs: ["empty={}"]}, { empty: {}}],
      [{ posArgs: ["two41={a: bcd, d: 3}"]}, {two41: {d: 3, a: "bcd"}}],
      [{ posArgs: ["turtles={all: {the: {way: down}}}", "flat={earth: [or, turtle, shell]}"]}, {turtles: {all: {the: {way: "down"}}}, flat: {earth: ["or", "turtle", "shell"]}}]
    ]
    expectFromCases(testCases)
  })
});
