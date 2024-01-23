import * as emit from "../../../../src/views/emit";
import MockedFunction = jest.MockedFunction;
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

    it.todo("sets next to be nextDate with just nextDate");
    it.todo("sets next to be nextTime with just nextTime");
    it.todo("combines nextDate and nextTime together if both are there");
    it.todo("still emits next if there is just createTime");
    it.todo("generates an ITI with the proper integral part if next is a date");
    it.todo("has the right integral part even with timezone shenanigans");
    it.todo(
      "uses the percentage through the day of occurrence as the decimal part of ITI if next is a date",
    );
    it.todo("uses the exact ITI if next is a time");
    it.todo("does not emit an ITI if there is no occurTime");
  });

  describe("reduce", () => {
    it.todo(
      "combines the values to determine the latest time, next, lastOccur, and iti",
    );
  });
});
