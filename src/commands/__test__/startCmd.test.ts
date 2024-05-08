import {
  deterministicHumanIds,
  restoreNow,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";
import { toDatumTime } from "../../time/timeUtils";
import { BadDurationError } from "../../errors";

describe("startCmd", () => {
  const dbName = "start_cmd_test";
  const db = testDbLifecycle(dbName);
  beforeEach(async () => {
    await setupCmd("");
    setNow("2023-09-02,12:30");
  });

  it("creates an occur document with state: true", async () => {
    const doc = await startCmd("dance");
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("can start from a different state", async () => {
    await switchCmd("machine preparing");
    setNow("+1");
    const doc = await startCmd("machine");
    expect(doc.data).toMatchObject({
      field: "machine",
      state: true,
      lastState: "preparing",
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("has an optional duration arg that creates a block of time where state was true", async () => {
    const doc = await startCmd("dance 5");
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      dur: "PT5M",
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("creates a hole of time where state was false when duration is negative", async () => {
    // Create an initial start of dancing in which to put a hole afterward
    await startCmd("dance -t -20");
    const doc = await startCmd("dance -5");
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      dur: "-PT5M",
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
    const doc = await startCmd("dance -m -k skillPoints 3");
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      skillPoints: 3,
      dur: null,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("when --omit-timestamp/-T is specified, there is no positional duration argument", async () => {
    const doc = await startCmd("dance -k skillPoints -T 3");
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      skillPoints: 3,
    });
    expect(doc.data).not.toHaveProperty("occurTime");
    expect(doc.data).not.toHaveProperty("dur");
  });

  it("can skip the duration if the duration is given as . or ''", async () => {
    // TODO: rewrite this test as a string based call;
    restoreNow();
    const doc = await startCmd("field -k optional . 50");
    const doc2 = await startCmd("field -k optional '' 50");
    expect(doc.data).toMatchObject({ field: "field", optional: 50 });
    expect(doc.data).not.toHaveProperty("dur");
    expect(doc2.data).toMatchObject({ field: "field", optional: 50 });
    expect(doc2.data).not.toHaveProperty("dur");
  });

  it("throws an error if the duration supplied is invalid", async () => {
    await expect(startCmd("field -k optional 30asd")).rejects.toThrow(
      BadDurationError,
    );
  });

  it("still assigns a state of true even with required keys, and duration comes before required keys", async () => {
    const doc = await startCmd("field 30 reqVal1 -K req1");
    expect(doc.data).toMatchObject({
      field: "field",
      dur: "PT30M",
      state: true,
      req1: "reqVal1",
    });
  });

  describe("change command", () => {
    deterministicHumanIds();

    beforeEach(async () => {
      setNow("2023-12-21 14:00");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command by having occur as a trailing word", async () => {
      expect(
        await startCmd("field 30 -k opt1= key=val optVal occur"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command by having start as a trailing word", async () => {
      expect(
        await startCmd("field -k opt1= 30 key=val optVal end"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command by having start as a trailing word", async () => {
      expect(
        await startCmd("field -k opt1= 5m30s key=val optVal switch stateName"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
});
