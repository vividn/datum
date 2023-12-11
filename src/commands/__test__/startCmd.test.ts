import { setNow, testDbLifecycle } from "../../__test__/test-utils";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";
import { toDatumTime } from "../../time/timeUtils";

describe("startCmd", () => {
  const dbName = "start_cmd_test";
  const db = testDbLifecycle(dbName);
  beforeEach(async () => {
    await setupCmd({});
    setNow("2023-09-02,12:30");
  });

  it("creates an occur document with state: true", async () => {
    const doc = await startCmd({ field: "dance" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("can start from a different state", async () => {
    await switchCmd({ field: "machine", state: "preparing" });
    setNow("+1");
    const doc = await startCmd({ field: "machine" });
    expect(doc.data).toMatchObject({
      field: "machine",
      state: true,
      lastState: "preparing",
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("has an optional duration arg that creates a block of time where state was true", async () => {
    const doc = await startCmd({ field: "dance", duration: "5" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      dur: "PT5M",
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("creates a hole of time where state was false when duration is negative", async () => {
    // Create an initial start of dancing in which to put a hole afterward
    await startCmd({ field: "dance", time: "-20" });
    const doc = await startCmd({ field: "dance", duration: "-5" });
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
    const doc = await startCmd({
      field: "dance",
      moment: true,
      optional: ["skillPoints"],
      duration: "3",
    });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      skillPoints: 3,
      dur: null,
      occurTime: toDatumTime(DateTime.now()),
    });
  });

  it("when --no-timestamp is specified, there is no positional duration argument", async () => {
    const doc = await startCmd({
      field: "dance",
      optional: ["skillPoints"],
      noTimestamp: true,
      duration: "3",
    });
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
    const doc = await startCmd({
      field: "field",
      moment: false,
      optional: "optional",
      duration: ".",
      data: [50],
    });
    const doc2 = await startCmd({
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
});
