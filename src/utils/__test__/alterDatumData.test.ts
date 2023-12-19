import { setNow } from "../../__test__/test-utils";
import { toDatumTime } from "../../time/timeUtils";
import { BadDateError, BadDurationError, BadTimeError } from "../../errors";
import SpyInstance = jest.SpyInstance;
import * as parseTimeStr from "../../time/parseTimeStr";
import * as parseDateStr from "../../time/parseDateStr";
import * as parseDurationStr from "../../time/parseDurationStr";
import { alterDatumData } from "../alterDatumData";
import { DatumData } from "../../documentControl/DatumDocument";
import { jClone } from "../jClone";

function expectAlterDatumData(
  input: Parameters<typeof alterDatumData>[0],
  expectedDatumData: DatumData,
): void {
  // alterDatumData changes the datumData in place, so we need to clone it
  const clonedInput = jClone(input);
  alterDatumData(clonedInput);
  expect(clonedInput.datumData).toEqual(expectedDatumData);
}

describe("alterDatumData", () => {
  const originalDatumData: DatumData = { existing: "data" };
  let datumData: DatumData = { ...originalDatumData };
  beforeEach(() => {
    datumData = { ...originalDatumData };
  });

  it("adds or replaces a key to an existing payload", () => {
    alterDatumData({
      datumData,
      path: "newKey",
      value: "value",
    });
    expect(datumData).toEqual({ existing: "data", newKey: "value" });
    alterDatumData({ datumData, path: "existing", value: "newData" });
    expect(datumData).toEqual({ existing: "newData", newKey: "value" });
  });

  it("can add a complex path to a payload", () => {
    alterDatumData({
      datumData,
      path: "nested.key",
      value: "value",
    });
    expect(datumData).toEqual({
      existing: "data",
      nested: { key: "value" },
    });
  });

  it("treats . paths as part of state", () => {
    alterDatumData({
      datumData,
      path: ".key",
      value: "value",
    });
    alterDatumData({
      datumData,
      path: ".otherKey",
      value: "value2",
    });
    expect(datumData).toEqual({
      existing: "data",
      state: { key: "value", otherKey: "value2" },
    });
  });

  it("interprets state as state.id path", () => {
    alterDatumData({
      datumData,
      path: "state",
      value: "value",
    });
    expect(datumData).toEqual({
      existing: "data",
      state: { id: "value" },
    });
  });

  it("parses . value as the default value for a key or undefined if no default is given", () => {
    expectAlterDatumData(
      { datumData, path: "key", value: "." },
      { ...datumData },
    );
    expectAlterDatumData(
      { datumData, path: "key", value: ".", defaultValue: "default" },
      { ...datumData, key: "default" },
    );
    expectAlterDatumData(
      { datumData, path: "key", value: ".", defaultValue: "3" },
      { ...datumData, key: 3 },
    );
    expectAlterDatumData(
      { datumData, path: "someDur", value: ".", defaultValue: "3" },
      { ...datumData, someDur: "PT3M" },
    );
  });

  it("can create or append to a key in the payload");
  it("calls inferType on the value passed in");

  describe("special fields", () => {
    beforeAll(() => {
      setNow("2021-10-25T22:30:00Z");
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
      alterDatumData({ datumData, path: "time", value: "3" });
      alterDatumData({
        datumData,
        path: "secondTime",
        value: "yesterday at 22:15",
      });
      alterDatumData({ datumData, path: "snake_time", value: "10:15" });
      alterDatumData({ datumData, path: "secondTime2", value: "1230" });

      expect(datumData).toEqual({
        existing: "data",
        firstTime: toDatumTime("2021-10-25T03:00:00.000Z"),
        secondTime: toDatumTime("2021-10-24T22:15:00.000Z"),
        snake_time: toDatumTime("2021-10-25T10:15:00.000Z"),
        secondTime2: toDatumTime("2021-10-25T12:30:00.000Z"),
      });
      expect(parseTimeSpy).toHaveBeenCalledTimes(4);
      expect(parseDateSpy).not.toHaveBeenCalled();
      expect(parseDurationSpy).not.toHaveBeenCalled();
    });

    it("infers values as dates if the field name is or ends in -Date", () => {
      expectAlterDatumData(datumData, "key", "-1", "date").toEqual(
        "2021-10-24",
      );
      expectAlterDatumData(datumData, "key", "dec21", "solsticeDate").toEqual(
        "2021-12-21",
      );
      expectAlterDatumData(datumData, "key", "in 3 days", "Due-Date").toEqual(
        "2021-10-28",
      );
      expectAlterDatumData(datumData, "key", "+2", "someDate10").toEqual(
        "2021-10-27",
      );

      expect(parseDateSpy).toHaveBeenCalledTimes(4);
      expect(parseTimeSpy).not.toHaveBeenCalled();
      expect(parseDurationSpy).not.toHaveBeenCalled();
    });

    it("infers values as durations if the field name is or ends in -Dur or -Duration", () => {
      expectAlterDatumData(datumData, "key", "3", "dur").toEqual("PT3M");
      expectAlterDatumData(datumData, "key", "-5", "Duration").toEqual("-PT5M");
      expectAlterDatumData(datumData, "key", "3:45:20", "raceDuration").toEqual(
        "PT3H45M20S",
      );
      expectAlterDatumData(datumData, "key", "2days", "wait_dur").toEqual(
        "P2D",
      );
      expectAlterDatumData(datumData, "key", "30sec", "duration2").toEqual(
        "PT30S",
      );
      expect(parseDurationSpy).toHaveBeenCalledTimes(5);

      parseDurationSpy.mockClear();
      expectAlterDatumData(datumData, "key", ".", "dur").toBeUndefined();
      expectAlterDatumData(datumData, "key", "", "dur").toBeUndefined();
      expect(parseDurationSpy).not.toHaveBeenCalled();

      expect(parseTimeSpy).not.toHaveBeenCalled();
      expect(parseDateSpy).not.toHaveBeenCalled();
    });

    it("throws an error for -Time -Date and -Dur values if they cannot be parsed", () => {
      expect(() =>
        alterDatumData(datumData, "key", "unparseable_time", "weirdTime"),
      ).toThrowError(BadTimeError);
      expect(parseTimeSpy).toHaveBeenCalled();
      expect(parseTimeSpy).not.toHaveReturned(); // because it threw an error

      expect(() =>
        alterDatumData(datumData, "key", "when pigs fly", "weirdDate"),
      ).toThrowError(BadDateError);
      expect(parseDateSpy).toHaveBeenCalled();
      expect(parseDateSpy).not.toHaveReturned();

      expect(() =>
        alterDatumData(
          datumData,
          "key",
          "as long as it takes",
          "weirdDuration",
        ),
      ).toThrowError(BadDurationError);
      expect(parseDurationSpy).toHaveBeenCalled();
      expect(parseDurationSpy).not.toHaveReturned();
    });

    it("does not call any of the special parse functions for other field names", () => {
      alterDatumData({ datumData, path: "tim", value: "3" });
      alterDatumData({ datumData, path: "mandate", value: "5" });
      alterDatumData({ datumData, path: "someField", value: "asdf" });
      alterDatumData({ datumData, path: "ptime", value: "3:45" });

      expect(parseTimeSpy).not.toHaveBeenCalled();
      expect(parseDateSpy).not.toHaveBeenCalled();
      expect(parseDurationSpy).not.toHaveBeenCalled();
    });
  });
});
