import {
  deterministicHumanIds,
  restoreNow,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { endCmd } from "../endCmd";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { switchCmd } from "../switchCmd";
import { startCmd } from "../startCmd";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";
import { DatumTime, toDatumTime } from "../../time/timeUtils";
import { BadDurationError } from "../../errors";

describe("endCmd", () => {
  const dbName = "end_cmd_test";
  const db = testDbLifecycle(dbName);
  beforeEach(async () => {
    await setupCmd({});
    setNow("2023-09-02,11:45");
  });

  it("creates an occur document with state: false", async () => {
    const doc = await endCmd({ field: "dance" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: false,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("can end from a different state", async () => {
    await switchCmd({ field: "activity", state: "cello" });
    setNow("+10");
    const doc = await endCmd({ field: "activity" });
    expect(doc.data).toMatchObject({
      field: "activity",
      state: false,
      lastState: "cello",
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("can create a hole of time when duration is present", async () => {
    // Create an initial start of dancing in which to put a hole afterward
    await startCmd({ field: "dance", time: "-20" });
    const doc = await endCmd({ field: "dance", duration: "5" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: false,
      dur: "PT5M",
      occurTime: toDatumTime(DateTime.now()),
    });
    const currentState = await getActiveState(db, "dance");
    expect(currentState).toBe(true);
    const intermediateState = await getActiveState(
      db,
      "dance",
      parseTimeStr({ timeStr: "-2.5" }),
    );
    expect(intermediateState).toBe(false);
  });

  it("when --moment is specified, dur is null and there is no duration postional argument", async () => {
    const doc = await endCmd({
      field: "dance",
      moment: true,
      optional: ["skillPoints"],
      duration: "3",
    });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: false,
      skillPoints: 3,
      dur: null,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("when --no-timestamp is specified, there is no positional duration argument", async () => {
    const doc = await endCmd({
      field: "dance",
      optional: ["skillPoints"],
      noTimestamp: true,
      duration: "3",
    });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: false,
      skillPoints: 3,
    });
    expect(doc.data).not.toHaveProperty("occurTime");
    expect(doc.data).not.toHaveProperty("dur");
  });

  it("can skip the duration if the duration is given as . or ''", async () => {
    // TODO: rewrite this test as a string based call;
    restoreNow();
    const doc = await endCmd({
      field: "field",
      moment: false,
      optional: "optional",
      duration: ".",
      data: [50],
    });
    const doc2 = await endCmd({
      field: "field",
      moment: false,
      optional: "optional",
      duration: "",
      data: [50],
    });
    expect(doc.data).toMatchObject({ field: "field", optional: 50 });
    expect(doc.data).not.toHaveProperty("dur");
    expect(doc2.data).toMatchObject({ field: "field", optional: 50 });
    expect(doc2.data).not.toHaveProperty("dur");
  });

  it("throws an error if the duration supplied is invalid", async () => {
    await expect(
      endCmd({
        field: "field",
        optional: "optional",
        duration: "30asd",
      }),
    ).rejects.toThrow(BadDurationError);
  });

  describe("change command", () => {
    deterministicHumanIds();

    let occurTime: DatumTime;
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
      occurTime = toDatumTime(DateTime.local());
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command by having occur as a trailing word", async () => {
      expect(
        await endCmd({
          field: "field",
          optional: ["opt1"],
          duration: "30",
          data: ["key=val", "optVal", "occur"],
        }),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become start command by having start as a trailing word", async () => {
      expect(
        await endCmd({
          field: "field",
          optional: ["opt1"],
          duration: "30",
          data: ["key=val", "optVal", "start"],
        }),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command by having start as a trailing word", async () => {
      expect(
        await endCmd({
          field: "field",
          optional: ["opt1"],
          duration: "5m30s",
          data: ["key=val", "optVal", "switch", "stateName"],
        }),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
});
