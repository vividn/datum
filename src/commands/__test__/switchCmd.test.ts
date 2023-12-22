import { deterministicHumanIds, restoreNow, setNow, testDbLifecycle } from "../../__test__/test-utils";
import { switchCmd } from "../switchCmd";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { DatumTime, toDatumTime } from "../../time/timeUtils";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";
import { startCmd } from "../startCmd";

describe("switchCmd", () => {
  const dbName = "switch_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({});
    setNow("2023-09-02,9:20");
  });
  afterEach(async () => {
    restoreNow();
  });

  it("creates an occur document with a custom state", async () => {
    const doc = await switchCmd({ field: "field", state: "newState" });
    expect(doc.data).toMatchObject({
      field: "field",
      state: "newState",
      occurTime: toDatumTime(DateTime.now()),
    });
    const dbDoc = await db.get(doc._id);
    expect(dbDoc).toEqual(doc);
  });

  it("records what it thinks the last state was", async () => {
    const doc = await switchCmd({ field: "environment", state: "outside" });
    expect(doc.data).toMatchObject({
      field: "environment",
      state: "outside",
      lastState: null,
    });
    setNow("+5");
    const doc2 = await switchCmd({ field: "environment", state: "inside" });
    expect(doc2.data).toMatchObject({
      field: "environment",
      state: "inside",
      lastState: "outside",
    });
  });

  it("can take in a manually specified last state", async () => {
    await switchCmd({ field: "project", state: "datum" });
    setNow("+5");
    const doc = await switchCmd({
      field: "project",
      state: "household",
      lastState: "german",
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      lastState: "german",
    });
  });

  it("does not include a lastState if there is no occurTime", async () => {
    await switchCmd({ field: "vacuum", state: true });
    const doc = await switchCmd({
      field: "vacuum",
      state: false,
      noTimestamp: true,
    });
    expect(doc.data).toMatchObject({
      field: "vacuum",
      state: false,
    });
    expect(doc.data).not.toHaveProperty("lastState");
    expect(doc.data).not.toHaveProperty("occurTime");
  });

  it("still includes lastState if manually specified even when there is no occurTime", async () => {
    await switchCmd({ field: "project", state: "linux" });
    const doc = await switchCmd({
      field: "project",
      state: "backend",
      lastState: "frontend",
      noTimestamp: true,
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: "backend",
      lastState: "frontend",
    });
    expect(doc.data).not.toHaveProperty("occurTime");
  });

  it("can do a state of null", async () => {
    const doc = await switchCmd({ field: "vacuum", state: null });
    const doc2 = await switchCmd({ field: "luft", state: "null" });
    expect(doc.data).toMatchObject({
      field: "vacuum",
      state: null,
    });
    expect(doc2.data).toMatchObject({
      field: "luft",
      state: null,
    });
  });

  it.todo(
    "assumes a state of true by default",
    // TODO: Write this once commands can be called by plain text in tests
  );

  it("records a block of state when specifying duration", async () => {
    const doc = await switchCmd({
      field: "project",
      state: "household",
      duration: "10m",
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      dur: "PT10M",
    });
    const intermediateState = await getActiveState(
      db,
      "project",
      parseTimeStr({ timeStr: "-5m" }),
    );
    expect(intermediateState).toBe("household");
  });

  it("records a hole of time in the state when given a negative duration", async () => {
    const doc = await switchCmd({
      field: "project",
      state: "household",
      duration: "-10m",
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      dur: "-PT10M",
    });
    const intermediateState = await getActiveState(
      db,
      "project",
      parseTimeStr({ timeStr: "-5m" }),
    );
    expect(intermediateState).toBe(false);
  });

  it("when --moment is specified, dur is null and there is no duration postional argument", async () => {
    const doc = await switchCmd({
      field: "project",
      state: "household",
      moment: true,
      optional: ["skillPoints"],
      duration: "3",
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      skillPoints: 3,
      dur: null,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("when --no-timestamp is specified, there is no positional duration argument", async () => {
    const doc = await switchCmd({
      field: "project",
      state: "household",
      optional: ["skillPoints"],
      noTimestamp: true,
      duration: "3",
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      skillPoints: 3,
    });
    expect(doc.data).not.toHaveProperty("occurTime");
    expect(doc.data).not.toHaveProperty("dur");
  });

  it("can skip the duration if the duration is given as . or ''", async () => {
    // TODO: rewrite this test as a string based call;
    restoreNow();
    const doc = await switchCmd({
      field: "project",
      state: "household",
      moment: false,
      optional: "optional",
      duration: ".",
      data: [50],
    });
    const doc2 = await switchCmd({
      field: "project",
      state: "household",
      moment: false,
      optional: "optional",
      duration: "",
      data: [50],
    });
    expect(doc.data).toMatchObject({ field: "project", optional: 50 });
    expect(doc.data).not.toHaveProperty("dur");
    expect(doc2.data).toMatchObject({ field: "project", optional: 50 });
    expect(doc2.data).not.toHaveProperty("dur");
  });

  it("can make complex states by using dot syntax", async () => {
    const doc = await switchCmd({
      field: "project",
      state: "household",
      data: [".subtask=mop"],
    });
    expect(doc.data).toMatchObject({
      field: "project",
      state: { id: "household", subtask: "mop" },
    });
  });

  it("does not record a lastState if there is no occurTime", async () => {
    await switchCmd({ state: "someState", field: "field"});
    setNow("+5");

    const secondDoc = await switchCmd({ state: "anotherState", field: "field", noTimestamp: true})
    expect(secondDoc.data).not.toHaveProperty("lastState");
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
        await switchCmd({
          field: "field",
          optional: ["opt1"],
          duration: "30",
          state: "someState",
          data: ["key=val", "optVal", "occur"],
        }),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command by having start as a trailing word", async () => {
      expect(
        await switchCmd({
          field: "field",
          optional: ["opt1"],
          duration: "30",
          state: "someState",
          data: ["key=val", "optVal", "end"],
        }),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a start command by having start as a trailing word", async () => {
      expect(
        await switchCmd({
          field: "field",
          optional: ["opt1"],
          duration: "5m30s",
          state: "someState",
          data: ["key=val", "optVal", "start"],
        }),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });

});
