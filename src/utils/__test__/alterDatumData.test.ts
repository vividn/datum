import { setNow } from "../../__test__/test-utils";
import { toDatumTime } from "../../time/timeUtils";
import { BadDateError, BadDurationError, BadTimeError } from "../../errors";
import SpyInstance = jest.SpyInstance;
import * as parseTimeStr from "../../time/parseTimeStr";
import * as parseDateStr from "../../time/parseDateStr";
import * as parseDurationStr from "../../time/parseDurationStr";
import { alterDatumData } from "../alterDatumData";
import { DatumData } from "../../documentControl/DatumDocument";
import * as inferTypeModule from "../inferType";

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

  it("calls inferType on the value passed in", () => {
    const inferTypeSpy = jest.spyOn(inferTypeModule, "inferType");
    alterDatumData({
      datumData,
      path: "newKey",
      value: "value",
    });
    expect(inferTypeSpy).toHaveBeenCalledWith("value");
  });

  it("parses . value as the default value for a key or undefined if no default is given", () => {
    alterDatumData({ datumData, path: "dotNoDefault", value: "." });
    alterDatumData({
      datumData,
      path: "dotWithDefault",
      value: ".",
      defaultValue: "default",
    });
    alterDatumData({
      datumData,
      path: "defaultIsInferred",
      value: ".",
      defaultValue: "3",
    });
    alterDatumData({
      datumData,
      path: "someDur",
      value: ".",
      defaultValue: "3",
    });
    expect(datumData).not.toHaveProperty("dotNoDefault");
    expect(datumData).toEqual({
      dotWithDefault: "default",
      defaultIsInferred: 3,
      someDur: "PT3M",
    });
  });

  it("can create or append to a key in the payload", () => {
    alterDatumData({
      datumData,
      path: "newKey",
      value: "value",
      append: true,
    });
    expect(datumData).toEqual({ existing: "data", newKey: "value" });
    alterDatumData({
      datumData,
      path: "existing",
      value: "newData",
      append: true,
    });
    expect(datumData).toEqual({
      existing: ["data", "newData"],
      newKey: ["value"],
    });
  });

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
      alterDatumData({ datumData, path: "date", value: "-1" });
      alterDatumData({ datumData, path: "solsticeDate", value: "dec21" });
      alterDatumData({ datumData, path: "Due-Date", value: "in 3 days" });
      alterDatumData({ datumData, path: "someDate10", value: "+2" });

      expect(datumData).toEqual({
        existing: "data",
        date: toDatumTime("2021-10-24"),
        solsticeDate: toDatumTime("2021-12-21"),
        DueDate: toDatumTime("2021-10-28"),
        someDate10: toDatumTime("2021-10-27"),
      });

      expect(parseDateSpy).toHaveBeenCalledTimes(4);
      expect(parseTimeSpy).not.toHaveBeenCalled();
      expect(parseDurationSpy).not.toHaveBeenCalled();
    });

    it("infers values as durations if the field name is or ends in -Dur or -Duration", () => {
      alterDatumData({ datumData, path: "dur", value: "3" });
      alterDatumData({ datumData, path: "Duration", value: "-5" });
      alterDatumData({ datumData, path: "raceDuration", value: "3:45:20" });
      alterDatumData({ datumData, path: "wait_dur", value: "2days" });
      alterDatumData({ datumData, path: "duration2", value: "30sec" });
      expect(parseDurationSpy).toHaveBeenCalledTimes(5);

      parseDurationSpy.mockClear();
      alterDatumData({ datumData, path: "dotDur", value: "." });
      alterDatumData({ datumData, path: "emptyDur", value: "" });
      expect(parseDurationSpy).not.toHaveBeenCalled();

      expect(parseTimeSpy).not.toHaveBeenCalled();
      expect(parseDateSpy).not.toHaveBeenCalled();

      expect(datumData).toEqual({
        dur: "PT3M",
        Duration: "-PT5M",
        raceDuration: "PT3H45M20S",
        wait_dur: "P2D",
        duration2: "PT30S",
      });
      expect(datumData).not.toHaveProperty("dotDur");
      expect(datumData).not.toHaveProperty("emptyDur");
    });

    it("throws an error for -Time -Date and -Dur values if they cannot be parsed", () => {
      expect(() =>
        alterDatumData({
          datumData,
          path: "weirdTime",
          value: "unparseable_time",
        }),
      ).toThrowError(BadTimeError);
      expect(parseTimeSpy).toHaveBeenCalled();
      expect(parseTimeSpy).not.toHaveReturned(); // because it threw an error

      expect(() =>
        alterDatumData({
          datumData,
          path: "weirdDate",
          value: "when pigs fly",
        }),
      ).toThrowError(BadDateError);
      expect(parseDateSpy).toHaveBeenCalled();
      expect(parseDateSpy).not.toHaveReturned();

      expect(() =>
        alterDatumData({
          datumData,
          path: "weirdDuration",
          value: "as long as it takes",
        }),
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
