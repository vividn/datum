import { setNow, testDbLifecycle } from "../../__test__/test-utils";
import { DateTime } from "luxon";
import { setupCmd } from "../setupCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";
import { getActiveState } from "../../state/getActiveState";
import { parseTimeStr } from "../../time/parseTimeStr";

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
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
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
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
    });
  });

  it("has an optional duration arg that creates a block of time where state was true", async () => {
    const doc = await startCmd({ field: "dance", duration: "5" });
    expect(doc.data).toMatchObject({
      field: "dance",
      state: true,
      dur: "PT5M",
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
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
      occurTime: {
        utc: DateTime.now().toUTC().toISO(),
      },
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
});
