import { setNow, testDbLifecycle } from "../../__test__/test-utils";
import { endCmd } from "../endCmd";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { switchCmd } from "../switchCmd";
import { startCmd } from "../startCmd";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";

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
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
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
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
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
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
    });
    const currentState = await getActiveState(db, "dance");
    expect(currentState).toBe(false);
    const intermediateState = await getActiveState(
      db,
      "dance",
      parseTimeStr({ timeStr: "-2.5" }),
    );
    expect(intermediateState).toBe(false);
  });
});
