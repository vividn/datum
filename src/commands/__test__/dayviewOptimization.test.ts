import { WatchingDayview } from "../../dayview/WatchingDayview";
import { DayviewCmdArgs } from "../dayviewCmd";
import { DateTime } from "luxon";
import { EitherPayload } from "../../documentControl/DatumDocument";

describe("WatchingDayview", () => {
  let watchingDayview: WatchingDayview;
  const mockArgs: DayviewCmdArgs = {
    nDays: 7,
    db: "test-db",
  };

  beforeEach(() => {
    watchingDayview = new WatchingDayview(mockArgs);
  });

  describe("determineDayFromChange", () => {
    const today = DateTime.local().toISODate();
    const yesterday = DateTime.local().minus({ days: 1 }).toISODate();
    const weekAgo = DateTime.local().minus({ days: 7 }).toISODate();

    beforeEach(async () => {
      await watchingDayview.initialize();
    });

    it("should return the correct day for a change with occurTime", () => {
      const change: PouchDB.Core.ChangesResponseChange<EitherPayload> = {
        id: "test-doc",
        seq: 1,
        changes: [{ rev: "1-abc" }],
        doc: {
          _id: "test-doc",
          _rev: "1-abc",
          data: {
            occurTime: {
              utc: today + "T10:00:00.000Z",
            },
            field: "test",
          },
          meta: {},
        },
      };

      const result = watchingDayview.determineDayFromChange(change);
      expect(result).toBe(today);
    });

    it("should return null for a change without occurTime", () => {
      const change: PouchDB.Core.ChangesResponseChange<EitherPayload> = {
        id: "test-doc",
        seq: 1,
        changes: [{ rev: "1-abc" }],
        doc: {
          _id: "test-doc",
          _rev: "1-abc",
          data: {
            field: "test",
          },
          meta: {},
        },
      };

      const result = watchingDayview.determineDayFromChange(change);
      expect(result).toBeNull();
    });

    it("should return null for a change with occurTime outside the displayed days", () => {
      const change: PouchDB.Core.ChangesResponseChange<EitherPayload> = {
        id: "test-doc",
        seq: 1,
        changes: [{ rev: "1-abc" }],
        doc: {
          _id: "test-doc",
          _rev: "1-abc",
          data: {
            occurTime: {
              utc: weekAgo + "T10:00:00.000Z",
            },
            field: "test",
          },
          meta: {},
        },
      };

      const result = watchingDayview.determineDayFromChange(change);
      expect(result).toBeNull();
    });

    it("should return null for a deleted document", () => {
      const change: PouchDB.Core.ChangesResponseChange<EitherPayload> = {
        id: "test-doc",
        seq: 1,
        changes: [{ rev: "1-abc" }],
        deleted: true,
      };

      const result = watchingDayview.determineDayFromChange(change);
      expect(result).toBeNull();
    });

    it("should handle data-only payload format", () => {
      const change: PouchDB.Core.ChangesResponseChange<EitherPayload> = {
        id: "test-doc",
        seq: 1,
        changes: [{ rev: "1-abc" }],
        doc: {
          _id: "test-doc",
          _rev: "1-abc",
          occurTime: {
            utc: yesterday + "T15:30:00.000Z",
          },
          field: "test",
        } as any,
      };

      const result = watchingDayview.determineDayFromChange(change);
      expect(result).toBe(yesterday);
    });
  });
});
