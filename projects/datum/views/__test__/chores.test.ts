import * as emit from "../../../../src/views/emit";
import SpyInstance = jest.SpyInstance;
import { makeDoc } from "../../../../src/__test__/test-utils";
import { ChoreDoc, choreView } from "../chores";
import { toDatumTime } from "../../../../src/time/timeUtils";

describe("choreView", () => {
  describe("map", () => {
    const field = "foo";
    const occurTime = toDatumTime("2024-01-23T15:00:00.000Z");
    const nextDate = "2024-01-25";
    const nextTime = toDatumTime("2024-01-26T13:00:00.000Z");
    const createTime = toDatumTime("2024-01-23T18:00:00.000Z");

    let emitMock: SpyInstance<void, [any, any]>;
    beforeEach(() => {
      emitMock = jest.spyOn(emit, "_emit");
    });

    it("emits nothing if there is no nextTime or nextDate", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime });
      choreView.map(doc);
      expect(emitMock).not.toHaveBeenCalled();
    });

    it("emits nothing if there is no metadata", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime, nextDate }, false);
      choreView.map(doc);
      expect(emitMock).not.toHaveBeenCalled();
    });

    it("emits nothing if there is no field", () => {
      // @ts-expect-error - intentionally missing field
      const doc = makeDoc<ChoreDoc>({ occurTime, nextDate });
      choreView.map(doc);
      expect(emitMock).not.toHaveBeenCalled();
    });

    it("emits occurTime as time and lastOccur if it exists", () => {
      const doc = makeDoc<ChoreDoc>(
        { field, occurTime, nextDate },
        { createTime },
      );
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      expect(emitMock).toHaveBeenCalledWith(
        field,
        expect.objectContaining({
          time: occurTime.utc,
          lastOccur: occurTime.utc,
        }),
      );
    });

    it("emits createTime as time if occurTime does not exist, with lastOccur set to zeroDate", () => {
      const doc = makeDoc<ChoreDoc>({ field, nextDate }, { createTime });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledWith(
        field,
        expect.objectContaining({
          time: createTime.utc,
          lastOccur: "0000-00-00",
        }),
      );
    });

    it("sets next to be nextDate with just nextDate", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime, nextDate });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      expect(emitMock).toHaveBeenCalledWith(
        field,
        expect.objectContaining({
          next: nextDate,
        }),
      );
    });

    it("sets next to be nextTime with just nextTime", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime, nextTime });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      expect(emitMock).toHaveBeenCalledWith(
        field,
        expect.objectContaining({
          next: nextTime.utc,
        }),
      );
    });

    it("combines nextDate and nextTime together if both are there", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime, nextDate, nextTime });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      expect(emitMock).toHaveBeenCalledWith(
        field,
        expect.objectContaining({
          next: `${nextDate}T${nextTime.utc.slice(11)}`,
        }),
      );
    });

    it("still emits next if there is just createTime", () => {
      const doc = makeDoc<ChoreDoc>({ field, nextDate }, { createTime });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      expect(emitMock).toHaveBeenCalledWith(
        field,
        expect.objectContaining({
          time: createTime.utc,
          next: nextDate,
        }),
      );
    });

    it("generates an ITI with the proper integral part if next is a date", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime, nextDate });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      const iti = emitMock.mock.calls[0][1].iti;
      expect(Math.floor(iti)).toBe(2);
    });

    it("has the right integral part even with timezone shenanigans", () => {
      const occurTime = toDatumTime("2024-01-23T18:00:00.000-12:00"); // the morning of the 24th in UTC
      const doc = makeDoc<ChoreDoc>({
        field,
        occurTime,
        nextDate: "2024-01-25",
      });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      const iti = emitMock.mock.calls[0][1].iti;
      expect(Math.floor(iti)).toBe(2); // based on local time
    });

    it("uses the percentage through the (local) day of occurrence as the decimal part of ITI if next is a date", () => {
      const occurTime = toDatumTime("2024-01-23T18:00:00.000-12:00");
      const doc = makeDoc<ChoreDoc>({
        field,
        occurTime,
        nextDate: "2024-01-25",
      });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      const iti = emitMock.mock.calls[0][1].iti;
      expect(iti - Math.floor(iti)).toEqual(18 / 24);
    });

    it("uses the exact ITI if next is a time", () => {
      const doc = makeDoc<ChoreDoc>({ field, occurTime, nextTime });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      const iti = emitMock.mock.calls[0][1].iti;
      const difference =
        (new Date(nextTime.utc).getTime() - new Date(occurTime.utc).getTime()) /
        (1000 * 60 * 60 * 24);
      expect(iti).toEqual(difference);
    });

    it("does not emit an ITI if there is no occurTime", () => {
      const doc = makeDoc<ChoreDoc>({ field, nextDate }, { createTime });
      choreView.map(doc);
      expect(emitMock).toHaveBeenCalledTimes(1);
      const iti = emitMock.mock.calls[0][1].iti;
      expect(iti).toBeUndefined();
    });
  });

  describe("reduce", () => {
    it.todo(
      "combines the values to determine the latest time, next, lastOccur, and iti",
    );
  });
});
