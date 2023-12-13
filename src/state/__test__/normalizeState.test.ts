import { normalizeState } from "../normalizeState";
import { mockedLogLifecycle } from "../../__test__/test-utils";

describe("unfoldState", () => {
  const { mockedWarn } = mockedLogLifecycle();

  it("returns undefined if state is undefined", () => {
    expect(normalizeState(undefined)).toBeUndefined();
  });

  it("returns null if state is null", () => {
    expect(normalizeState({ id: null })).toBe(null);
    expect(normalizeState(null)).toBe(null);
  });

  it("turns a non object state into a state with the corresponding id", () => {
    expect(normalizeState("abcd")).toBe({ id: "abcd" });
    expect(normalizeState(false)).toBe({ id: false });
    expect(normalizeState(true)).toBe({ id: true });
    expect(normalizeState(123)).toBe({ id: 123 });
  });

  it("keeps an object state with an id as it is", () => {
    expect(normalizeState({ id: "id", abc: 123, key: "value" })).toBe({
      id: "id",
      abc: 123,
      key: "value",
    });
  });

  it("assigns a state id of true if it is not defined in an object", () => {

  })

  it("unfolds an id into the state if id is an object");

  it("interpolates other fields into an unfolded id, overwriting as necessary")

  it("turns an array state into an array of object states with the corresponding ids", () => {
    expect(normalizeState(["abcd", false, true])).toEqual([
      { id: "abcd" },
      { id: false },
      { id: true },
    ]);
  });

  it("keeps the object states in the array intact", () => {
    expect(
      normalizeState([
        { id: "abcd", name: "abc" },
        "stringState",
        { no: "id", gets: "true id" },
      ]),
    ).toEqual([
      { id: "abcd", name: "abc" },
      { id: "stringState" },
      { id: true, no: "id", gets: "true id" },
    ]);
  });

  it("recursively unfolds state if id is an object with just id", () => {
    expect(normalizeState({ id: { id: "abc" } })).toEqual("abc");
  });

  it("leaves a complex state with a string id, or no id as it is", () => {
    expect(normalizeState({ id: "abc", name: "name" })).toEqual({
      id: "abc",
      name: "name",
    });
    expect(normalizeState({ name: "name", n: 3 })).toEqual({
      name: "name",
      n: 3,
    });
  });

  it("turns a complex state with id=true into the complex state without id", () => {
    expect(normalizeState({ id: true, name: "name", n: 3 })).toEqual({
      name: "name",
      n: 3,
    });
  });

  it("turns a complex state with id=null into just null, and warns", () => {
    expect(normalizeState({ id: null, name: "name", n: 3 })).toBe(null);
    expect(mockedWarn).toHaveBeenCalledTimes(1);
    expect(mockedWarn.mock.calls[0][0]).toBe(
      "unfoldState: id is null, returning null",
    );
  });

  it.todo("turns a complex state with id=false into just false, and warns");

  it("unfolds an array of ids into an array of objects with the other fields in all of them", () => {
    expect(normalizeState({ id: ["state1", "state2", "state3"] })).toEqual([
      ["state1", "state2", "state3"],
    ]);
    expect(
      normalizeState({
        id: ["state1", "state2", true],
        name: "name",
        n: 3,
      }),
    ).toEqual([
      { id: "state1", name: "name", n: 3 },
      { id: "state2", name: "name", n: 3 },
      { name: "name", n: 3 },
    ]);
  });

  it.todo("throws an error if a null state is part of an array of states");
});
