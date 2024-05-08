import { consolidateKeys } from "../consolidateKeys";

describe("consolidateKeys", () => {
  it("if a key is used multiple times, the last appearing form (required/optional/default) is used in the first appearing position", () => {
    const consolidated = consolidateKeys([
      "a",
      "b",
      "a=2",
      "c",
      "d=abcd",
      "a=4",
      "d",
      "c=",
      "e=",
      "f",
      "f=",
      "f",
    ]);
    expect(consolidated).toEqual(["a=4", "b", "c=", "d", "e=", "f"]);
  });
});
