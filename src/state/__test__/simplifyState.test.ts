import { simplifyState } from "../simplifyState";
describe("simplifyState", () => {
  it("returns a simplified state", () => {
    expect(simplifyState(null)).toBeNull();
    expect(simplifyState("state1")).toBe("state1");
    expect(simplifyState(true)).toBe(true);
    expect(simplifyState(false)).toBe(false);
    expect(simplifyState(["state1"])).toEqual("state1");
    expect(simplifyState(["state1", "state2"])).toEqual(["state1", "state2"]);
    expect(simplifyState({ id: "state1", extra: "data" })).toBe("state1");
    expect(
      simplifyState([
        { id: "complex", multi: "state" },
        { id: "should", be: "array", of: "ids" },
      ]),
    ).toEqual(["complex", "should"]);
  });
});
