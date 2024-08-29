import { normalizeState } from "../normalizeState";
import { mockedLogLifecycle } from "../../__test__/test-utils";
import { BadStateError } from "../../errors";

describe("unfoldState", () => {
  const { mockedWarn } = mockedLogLifecycle();

  it("returns null if state is null", () => {
    expect(normalizeState({ id: null })).toEqual(null);
    expect(normalizeState(null)).toEqual(null);
  });

  it("returns non object state without modification", () => {
    expect(normalizeState("abcd")).toEqual("abcd");
    expect(normalizeState(false)).toEqual(false);
    expect(normalizeState(true)).toEqual(true);
  });

  it("stringifys number states", () => {
    expect(normalizeState(123)).toEqual("123");
  });

  it("unfolds objects with just id into their simple value counterparts", () => {
    expect(normalizeState({ id: true })).toEqual(true);
    expect(normalizeState({ id: false })).toEqual(false);
    expect(normalizeState({ id: 123 })).toEqual("123");
    expect(normalizeState({ id: null })).toEqual(null);
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
    expect(normalizeState({})).toEqual(true);
  });

  it("unfolds an id into the state if id is an object", () => {
    expect(normalizeState({ id: { abc: 123, key: "value" } })).toEqual({
      id: true,
      abc: 123,
      key: "value",
    });
    expect(normalizeState({ id: { id: "abc" } })).toEqual("abc");
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

  it("recursively normalizes an array state", () => {
    expect(
      normalizeState(["abcd", { id: "stringId" }, true, { abc: "def" }]),
    ).toEqual(["abcd", "stringId", true, { id: true, abc: "def" }]);
  });

  it("keeps a complex state with id=true the same", () => {
    expect(normalizeState({ id: true, name: "name", n: 3 })).toEqual({
      id: true,
      name: "name",
      n: 3,
    });
  });

  it("turns a complex state with id=null into just {null, and warns", () => {
    expect(normalizeState({ id: null, name: "name", n: 3 })).toEqual(null);
    expect(mockedWarn).toHaveBeenCalledTimes(1);
    expect(mockedWarn.mock.calls[0][0]).toMatchSnapshot();
  });

  it("turns an array of length 0 into a false state", () => {
    expect(normalizeState([])).toEqual(false);
    expect(normalizeState({ id: [] })).toEqual(false);
    expect(normalizeState({ id: [], name: "john" })).toEqual({
      id: false,
      name: "john",
    });
  });

  it("unfolds an object with an array of ids into an array of objects with the other fields in all of them", () => {
    expect(normalizeState({ id: ["state1", "state2", "state3"] })).toEqual([
      "state1",
      "state2",
      "state3",
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
    ).toEqual(["state1", "state2", "state3", "state4", "state5"]);
    expect(
      normalizeState({
        id: [
          "state1",
          { id: "state2" },
          { id: ["state3", ["state4", "state5"]], name: "john" },
        ],
      }),
    ).toEqual([
      "state1",
      "state2",
      { id: "state3", name: "john" },
      { id: "state4", name: "john" },
      { id: "state5", name: "john" },
    ]);
  });

  it("turns an input array of length 1 into an non array", () => {
    expect(normalizeState(["abcd"])).toEqual("abcd");
    expect(normalizeState([{ id: "abcd", name: "john" }])).toEqual({
      id: "abcd",
      name: "john",
    });
    expect(normalizeState([{ id: null }])).toEqual(null);
    expect(normalizeState([null])).toEqual(null);
  });

  it("throws an error if a null state is part of an array of states", () => {
    expect(() => normalizeState(["abc", null])).toThrow(BadStateError);
    expect(() => normalizeState([null, "abc"])).toThrow(BadStateError);
    expect(() => normalizeState([null, null])).toThrow(BadStateError);
    expect(() => normalizeState(["abc", { id: null }])).toThrow(BadStateError);
    expect(() => normalizeState([null, { id: "abc" }])).toThrow(BadStateError);
    expect(() => normalizeState([{ id: "abc" }, { id: null }])).toThrow(
      BadStateError,
    );
  });

  it("converts a state of 'start' to true", () => {
    expect(normalizeState("start")).toBe(true);
    expect(normalizeState(["a", "start"])).toEqual(["a", true]);
    expect(normalizeState({ id: "start", extra: "key" })).toEqual({
      id: true,
      extra: "key",
    });
  });

  it("converts a state of 'end' to false", () => {
    expect(normalizeState("end")).toBe(false);
    expect(normalizeState(["a", "end"])).toEqual(["a", false]);
    expect(normalizeState({ id: "end", extra: "key" })).toEqual({
      id: false,
      extra: "key",
    });
  });
});
