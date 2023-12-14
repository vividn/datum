import { normalizeState } from "../normalizeState";
import { mockedLogLifecycle } from "../../__test__/test-utils";
import { BadStateError } from "../../errors";

describe("unfoldState", () => {
  const { mockedWarn } = mockedLogLifecycle();

  it("returns {id: null} if state is null", () => {
    expect(normalizeState({ id: null })).toEqual({ id: null });
    expect(normalizeState(null)).toEqual({ id: null });
  });

  it("turns a non object state into a state with the corresponding id", () => {
    expect(normalizeState("abcd")).toEqual({ id: "abcd" });
    expect(normalizeState(false)).toEqual({ id: false });
    expect(normalizeState(true)).toEqual({ id: true });
    expect(normalizeState(123)).toEqual({ id: 123 });
  });

  it("keeps an object state with an id as it is", () => {
    expect(normalizeState({ id: "id", abc: 123, key: "value" })).toEqual({
      id: "id",
      abc: 123,
      key: "value",
    });
  });

  it("assigns a state id of true if it is not defined in an object", () => {
    expect(normalizeState({ abc: 123, key: "value" })).toEqual({
      id: true,
      abc: 123,
      key: "value",
    });
    expect(normalizeState({})).toEqual({
      id: true,
    });
  });

  it("unfolds an id into the state if id is an object", () => {
    expect(normalizeState({ id: { abc: 123, key: "value" } })).toEqual({
      id: true,
      abc: 123,
      key: "value",
    });
    expect(normalizeState({ id: { id: "abc" } })).toEqual({ id: "abc" });
    expect(
      normalizeState({ id: { id: "stringId", abc: 123, key: "value" } }),
    ).toEqual({
      id: "stringId",
      abc: 123,
      key: "value",
    });
  });

  it("interpolates other fields into an unfolded id, overwriting as necessary", () => {
    expect(
      normalizeState({
        id: { id: "stringId", abc: 123, key: "value" },
        key: "replacementValue",
      }),
    ).toEqual({
      id: "stringId",
      abc: 123,
      key: "replacementValue",
    });
  });

  it("recursively unfolds nested ids", () => {
    expect(
      normalizeState({
        id: {
          id: { id: "deepId", key: "value1", abc: 123, untouched: "inner_key" },
          key: "value2",
          nested: { inner: "key" },
        },
        key: "value3",
        abc: 456,
      }),
    ).toEqual({
      id: "deepId",
      key: "value3",
      abc: 456,
      untouched: "inner_key",
      nested: { inner: "key" },
    });
  });

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

  it("keeps a complex state with id=true the same", () => {
    expect(normalizeState({ id: true, name: "name", n: 3 })).toEqual({
      id: true,
      name: "name",
      n: 3,
    });
  });

  it("turns a complex state with id=null into just {id: null}, and warns", () => {
    expect(normalizeState({ id: null, name: "name", n: 3 })).toEqual({ id: null });
    expect(mockedWarn).toHaveBeenCalledTimes(1);
    expect(mockedWarn.mock.calls[0][0]).toMatchSnapshot();
  });

  it("turns an array of length 0 into a false state", () => {
    expect(normalizeState([])).toEqual({ id: false });
    expect(normalizeState({ id: [] })).toEqual({ id: false });
    expect(normalizeState({ id: [], name: "john" })).toEqual({
      id: false,
      name: "john",
    });
  });

  it("unfolds an array of values into an array of objects with the corresponding ids", () => {
    expect(normalizeState(["abcd", 123, true])).toEqual([
      { id: "abcd" },
      { id: 123 },
      { id: true },
    ]);
    expect(
      normalizeState(["abcd", { id: "specified", name: "john" }, { no: "id" }]),
    ).toEqual([
      { id: "abcd" },
      { id: "specified", name: "john" },
      { id: true, no: "id" },
    ]);
  });

  it("unfolds an object with an array of ids into an array of objects with the other fields in all of them", () => {
    expect(normalizeState({ id: ["state1", "state2", "state3"] })).toEqual([
      { id: "state1" },
      { id: "state2" },
      { id: "state3" },
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
      { id: true, name: "name", n: 3 },
    ]);
  });

  it("flattens nested arrays of state", () => {
    expect(
      normalizeState([
        { id: ["state1", "state2", "state3"] },
        { id: "state4" },
        "state5",
      ]),
    ).toEqual([
      { id: "state1" },
      { id: "state2" },
      { id: "state3" },
      { id: "state4" },
      { id: "state5" },
    ]);
    expect(
      normalizeState({
        id: [
          "state1",
          { id: "state2" },
          { id: ["state3", ["state4", "state5"]], name: "john" },
        ],
      }),
    ).toEqual([
      { id: "state1" },
      { id: "state2" },
      { id: "state3", name: "john" },
      { id: "state4", name: "john" },
      { id: "state5", name: "john" },
    ]);
  });

  it("turns an input array of length 1 into an non array", () => {
    expect(normalizeState(["abcd"])).toEqual({ id: "abcd" });
    expect(normalizeState([{ id: "abcd", name: "john" }])).toEqual({
      id: "abcd",
      name: "john",
    });
    expect(normalizeState([{ id: null }])).toEqual({ id: null });
    expect(normalizeState([null])).toEqual({ id: null });
  });

  it("throws an error if a null state is part of an array of states", () => {
    expect(() => normalizeState(["abc", null])).toThrowError(BadStateError);
    expect(() => normalizeState([null, "abc"])).toThrowError(BadStateError);
    expect(() => normalizeState([null, null])).toThrowError(BadStateError);
    expect(() => normalizeState(["abc", { id: null }])).toThrowError(
      BadStateError,
    );
    expect(() => normalizeState([null, { id: "abc" }])).toThrowError(
      BadStateError,
    );
    expect(() => normalizeState([{ id: "abc" }, { id: null }])).toThrowError(
      BadStateError,
    );
  });
});
