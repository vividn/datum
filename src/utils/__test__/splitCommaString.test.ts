import { splitCommaString } from "../splitCommaString";

describe("splitCommaString", () => {
  it("should split a string based on non escaped commas", () => {
    expect(splitCommaString("a,b,c")).toEqual(["a", "b", "c"]);
    expect(splitCommaString("a\\,b,c")).toEqual(["a,b", "c"]);
    expect(splitCommaString("a,b\\,c")).toEqual(["a", "b,c"]);
    expect(splitCommaString("a\\,b\\,c")).toEqual("a,b,c");
    expect(splitCommaString("a\\,b\\,c\\,")).toEqual("a,b,c,");
    expect(splitCommaString("a,b,c,")).toEqual(["a", "b", "c"]);
    expect(splitCommaString(",a,b,c")).toEqual(["a", "b", "c"]);
    expect(splitCommaString(",a,b,c,")).toEqual(["a", "b", "c"]);
    expect(splitCommaString("a")).toEqual("a");
    expect(splitCommaString("a,")).toEqual(["a"]);
    expect(splitCommaString("")).toEqual("");
    expect(splitCommaString(",")).toEqual([]);
  });
});
