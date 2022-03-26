import { inferType } from "../inferType";
import * as parseTimeStr from "../../time/parseTimeStr";
import * as parseDateStr from "../../time/parseDateStr";
import * as parseDurationStr from "../../time/parseDurationString";
import { DateTime, Settings } from "luxon";
import SpyInstance = jest.SpyInstance;

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
  beforeAll(() => {
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

  let parseTimeSpy: SpyInstance,
    parseDateSpy: SpyInstance,
    parseDurationSpy: SpyInstance;
  beforeEach(() => {
    parseTimeSpy = jest.spyOn(parseTimeStr, "parseTimeStr");
    parseDateSpy = jest.spyOn(parseDateStr, "parseDateStr");
    parseDurationSpy = jest.spyOn(parseDurationStr, "parseDurationStr");
  });

  it("infers values as datetimes if the field name is or ends in -Time", () => {
    expect(inferType("3", "time")).toEqual("2021-10-25T03:00:00.000Z");
    expect(inferType("yesterday at 22:15", "expectedTime")).toEqual(
      "2021-10-24T22:15:00.000Z"
    );
    expect(inferType("10:15", "snake_time")).toEqual(
      "2021-10-25T10:15:00.000Z"
    );

    expect(parseTimeSpy).toHaveBeenCalledTimes(3);
    expect(parseDateSpy).not.toHaveBeenCalled();
    expect(parseDurationSpy).not.toHaveBeenCalled();
  });

  it("infers values as dates if the field name is or ends in -Date", () => {
    expect(inferType("-1", "date")).toEqual("2021-10-24");
    expect(inferType("dec21", "solsticeDate")).toEqual("2021-12-21");
    expect(inferType("in 3 days", "Due-Date")).toEqual("2021-10-28");

    expect(parseDateSpy).toHaveBeenCalledTimes(3);
    expect(parseTimeSpy).not.toHaveBeenCalled();
    expect(parseDurationSpy).not.toHaveBeenCalled();
  });

  it("infers values as durations if the field name is or ends in -Dur or -Duration", () => {
    expect(inferType("3", "dur")).toEqual("PT3M");
    expect(inferType("-5", "Duration")).toEqual("-PT5M");
    expect(inferType("3:45:20", "raceDuration")).toEqual("PT3H45M20S");
    expect(inferType("2days", "wait_dur")).toEqual("P2D");

    expect(parseDurationSpy).toHaveBeenCalledTimes(4);
    expect(parseTimeSpy).not.toHaveBeenCalled();
    expect(parseDateSpy).not.toHaveBeenCalled();
  });

  it("leaves the value as a string for -Time -Date and -Dur values if they cannot be parsed", () => {
    expect(inferType("unparseable_time", "weirdTime")).toEqual(
      "unparseable_time"
    );
    expect(parseTimeSpy).toHaveBeenCalled();
    expect(parseTimeSpy).not.toHaveReturned(); // because it threw an error

    expect(inferType("when pigs fly", "weirdDate")).toEqual("when pigs fly");
    expect(parseDateSpy).toHaveBeenCalled();
    expect(parseDateSpy).not.toHaveReturned();

    expect(inferType("as long as it takes", "weirdDuration")).toEqual(
      "as long as it takes"
    );
    expect(parseDurationSpy).toHaveBeenCalled();
    expect(parseDurationSpy).not.toHaveReturned();
  });

  it("does not call any of the special parse functions for other field names", () => {
    inferType("3", "tim");
    inferType("5", "mandate");
    inferType("asdf", "someField");
    inferType("3:45", "ptime");

    expect(parseTimeSpy).not.toHaveBeenCalled();
    expect(parseDateSpy).not.toHaveBeenCalled();
    expect(parseDurationSpy).not.toHaveBeenCalled();
  });
});
