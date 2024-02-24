import {
  deterministicHumanIds,
  restoreNow,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { switchCmd } from "../switchCmd";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { toDatumTime } from "../../time/timeUtils";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";

describe("switchCmd", () => {
  const dbName = "switch_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
    setNow("2023-09-02,9:20");
  });
  afterEach(async () => {
    restoreNow();
  });

  it("creates an occur document with a custom state", async () => {
    const doc = await switchCmd("field newState");
    expect(doc.data).toMatchObject({
      field: "field",
      state: "newState",
      occurTime: toDatumTime(DateTime.now()),
    });
    const dbDoc = await db.get(doc._id);
    expect(dbDoc).toEqual(doc);
  });

  it("records what it thinks the last state was", async () => {
    const doc = await switchCmd("environment outside");
    expect(doc.data).toMatchObject({
      field: "environment",
      state: "outside",
      lastState: null,
    });
    setNow("+5");
    const doc2 = await switchCmd("environment inside");
    expect(doc2.data).toMatchObject({
      field: "environment",
      state: "inside",
      lastState: "outside",
    });
  });

  it("can take in a manually specified last state", async () => {
    await switchCmd("project datum");
    setNow("+5");
    const doc = await switchCmd("project household --last-state german");
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      lastState: "german",
    });
  });

  it("does not include a lastState if there is no occurTime", async () => {
    await switchCmd("vacuum true");
    const doc = await switchCmd("vacuum false --omit-timestamp");
    expect(doc.data).toMatchObject({
      field: "vacuum",
      state: false,
    });
    expect(doc.data).not.toHaveProperty("lastState");
    expect(doc.data).not.toHaveProperty("occurTime");
  });

  it("still includes lastState if manually specified even when there is no occurTime", async () => {
    await switchCmd("project linux");
    const doc = await switchCmd("project backend -L frontend -T");
    expect(doc.data).toMatchObject({
      field: "project",
      state: "backend",
      lastState: "frontend",
    });
    expect(doc.data).not.toHaveProperty("occurTime");
  });

  it("can do a state of null", async () => {
    const doc = await switchCmd("vacuum null");
    const doc2 = await switchCmd("luft null");
    expect(doc.data).toMatchObject({
      field: "vacuum",
      state: null,
    });
    expect(doc2.data).toMatchObject({
      field: "luft",
      state: null,
    });
  });

  it("assumes a state of true by default", async () => {
    const doc = await switchCmd("eat");
    expect(doc.data.state).toBe(true);
  });

  it("records a block of state when specifying duration", async () => {
    const doc = await switchCmd("project household 10m");
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      dur: "PT10M",
    });
    const intermediateState = await getActiveState(
      db,
      "project",
      parseTimeStr({ timeStr: "-5m" })
    );
    expect(intermediateState).toBe("household");
  });

  it("records a hole of time in the state when given a negative duration", async () => {
    const doc = await switchCmd("project household -10");
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      dur: "-PT10M",
    });
    const intermediateState = await getActiveState(
      db,
      "project",
      parseTimeStr({ timeStr: "-5m" })
    );
    expect(intermediateState).toBe(false);
  });

  it("when --moment/-m is specified, dur is null and there is no duration postional argument", async () => {
    const doc = await switchCmd("project household -m -k skillPoints 3");
    expect(doc.data).toMatchObject({
      field: "project",
      state: "household",
      skillPoints: 3,
      dur: null,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("when --omit-timestamp/-T is specified, there is no positional duration argument", async () => {
    const doc = await switchCmd("project household -k skillPoints -T 3");
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
    const doc = await switchCmd("project household -k optional . 50");
    const doc2 = await switchCmd("project household -k optional '' 50");
    expect(doc.data).toMatchObject({ field: "project", optional: 50 });
    expect(doc.data).not.toHaveProperty("dur");
    expect(doc2.data).toMatchObject({ field: "project", optional: 50 });
    expect(doc2.data).not.toHaveProperty("dur");
  });

  it("can make complex states by using dot syntax", async () => {
    const doc = await switchCmd("project household .subtask=mop");
    expect(doc.data).toMatchObject({
      field: "project",
      state: { id: "household", subtask: "mop" },
    });
  });

  it("does not record a lastState if there is no occurTime", async () => {
    await switchCmd("field someState");
    setNow("+5");

    const secondDoc = await switchCmd("field anotherState -T");
    expect(secondDoc.data).not.toHaveProperty("lastState");
  });

  it("handles required keys and optional keys for complex state correctly", async () => {
    const doc = await switchCmd(
      "book -K .title -K .author -k .genre 'the wind in the willows' 'kenneth grahame' . 5 fiction"
    );

    expect(doc.data).toMatchObject({
      field: "book",
      state: {
        title: "the wind in the willows",
        author: "kenneth grahame",
        genre: "fiction",
      },
      dur: "PT5M",
    });
  });

  it("handles dot syntax required and optional keys correctly", async () => {
    const doc = await switchCmd(
      "consume -K .medium -k .title -k .author .medium=text book_fiction . title author"
    );
    expect(doc.data).toMatchObject({
      field: "consume",
      state: {
        medium: "text",
        id: "book_fiction",
        title: "title",
        author: "author",
      },
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
        await switchCmd("field -k opt1 someState 30 key=val optVal occur")
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command by having start as a trailing word", async () => {
      expect(
        await switchCmd("field -k opt1 someState 30 key=val optVal end")
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a start command by having start as a trailing word", async () => {
      expect(
        await switchCmd("field -k opt1 someState 5m30s key=val optVal start")
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
});
