import { describe, expect, it } from "@jest/globals";
import { splitFirst } from "../splitFirst";

describe("splitFirst", () => {
  const splitFirstEquals = (str: string) => splitFirst("=", str);
  it("returns [str] if there are no equals signs", () => {
    expect(splitFirstEquals("")).toStrictEqual([""]);
    expect(splitFirstEquals("a")).toStrictEqual(["a"]);
    expect(splitFirstEquals("a,bsdflkj3")).toStrictEqual(["a,bsdflkj3"]);
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

  it("can also use other separators", () => {
    expect(splitFirst(":", "no_colons_here")).toStrictEqual(["no_colons_here"]);
    expect(splitFirst(":", "a:b:c")).toStrictEqual(["a", "b:c"]);
    expect(splitFirst(":", "a:::::")).toStrictEqual(["a", "::::"]);
    expect(splitFirst("/", "a/b/c")).toStrictEqual(["a", "b/c"]);
  });
});