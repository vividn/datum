import { describe, expect, it } from "@jest/globals";
import inferType from "../inferType";
import * as parseTimeStr from "../../time/parseTimeStr";
import * as parseDateStr from "../../time/parseDateStr";
import * as parseDurationStr from "../../time/parseDurationString";
import timezone_mock from "timezone-mock";
import { DateTime, Settings } from "luxon";

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
    expect(inferType("nan")).toBe("NaN");
    expect(inferType("NaN")).toBe("NaN");
    expect(inferType("NAN")).toBe("NaN");

    expect(inferType("null")).toBe(null);
    expect(inferType("NULL")).toBe(null);

    expect(inferType("inf")).toBe("Infinity");
    expect(inferType("infinity")).toBe("Infinity");
    expect(inferType("-inf")).toBe("-Infinity");
    expect(inferType("-infinity")).toBe("-Infinity");
  });

  it("converts array looking data", () => {
    expect(inferType("[3, 4, 5]")).toEqual([3, 4, 5]);
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

describe("inferType with special fields", () => {
  const parseTimeSpy = jest.spyOn(parseTimeStr, "default");
  const parseDateSpy = jest.spyOn(parseDateStr, "default");
  const parseDurationSpy = jest.spyOn(parseDurationStr, "default");

  beforeEach(() => {
    jest.clearAllMocks();
  });

  beforeAll(() => {
    timezone_mock.register("UTC");
    const mockNow = DateTime.fromObject({
      year: 2021,
      month: 10,
      day: 25,
      hour: 22,
      minute: 30,
    });
    const mockNowMillis = mockNow.toMillis();
    Settings.now = () => mockNowMillis;
  });

  afterAll(() => {
    jest.restoreAllMocks();
    timezone_mock.unregister();
    Settings.resetCaches();
  });

  it("infers values as datetimes if the field name is or ends in -Time", () => {
    expect(inferType("3", "time")).toEqual("2021-10-25T03:00:00.000Z");
    expect(inferType("-15", "expectedTime")).toEqual(
      "2021-10-25T22:15:00.000Z"
    );
    expect(inferType("10:15", "snake_time")).toEqual(
      "2021-10-25T03:00:00.000Z"
    );
    expect(parseTimeSpy).toHaveBeenCalledTimes(3);
  });

  it("infers values as dates if the field name is or ends in -Date", () => {
    expect(inferType("-1", "date")).toEqual("2021-10-24");
    expect(inferType("dec21", "solsticeDate")).toEqual("2021-12-21");
    expect(inferType("in 3 days", "Due-Date")).toEqual("2021-10-28");
  });

  it.todo(
    "infers values as durations if the field name is or dends in -Dur or -Duration"
  );

  it.todo(
    "leaves the value as a string for -Time -Date and -Dur values if they cannot be parsed"
  );

  it.todo(
    "does not call any of the special parse functions for other field names"
  );
});
