import { restoreNow, setNow, testDbLifecycle } from "../../test-utils";
import { switchCmd } from "../switchCmd";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";

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
      occurTime: DateTime.now().toUTC().toISO(),
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
    const doc2 = await switchCmd({ field: "luft", state: "null"})
    expect(doc.data).toMatchObject({
      field: "vacuum",
      state: null,
    });
    expect(doc2.data).toMatchObject({
      field: "luft",
      state: null,
    });
  });
});
