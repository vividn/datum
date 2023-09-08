import { at, restoreNow, setNow, testDbLifecycle } from "../../test-utils";
import { setupCmd } from "../../commands/setupCmd";
import { getActiveState } from "../getActiveState";
import { switchCmd } from "../../commands/switchCmd";
import { startCmd } from "../../commands/startCmd";
import { endCmd } from "../../commands/endCmd";
import { DateTime } from "luxon";

describe("getActiveState", () => {
  const dbName = "get_active_state_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({ db: dbName });
  });

  afterEach(() => {
    restoreNow();
  });

  it("returns null if no state has ever been recorded", async () => {
    const activeState = await getActiveState(db, "field");
    expect(activeState).toBe(null);
  });

  it("returns null even if another field has been changed to a state (both before and after alphabetically)", async () => {
    await switchCmd({ db: dbName, field: "fald", state: "active" });
    await switchCmd({ db: dbName, field: "fold", state: "someState" });
    const activeState = await getActiveState(db, "field");
    expect(activeState).toBe(null);
  });

  it("after switching to a state the active state is that new state", async () => {
    await switchCmd({ db: dbName, field: "field", state: "active" });
    const activeState = await getActiveState(db, "field");
    expect(activeState).toBe("active");
  });
  it("after switching to a state the active state is that new state even if another field has been changed", async () => {
    await switchCmd({ db: dbName, field: "field", state: "active" });
    await switchCmd({ db: dbName, field: "different", state: "anotherState" });
    const activeState = await getActiveState(db, "field");
    const differentState = await getActiveState(db, "different");
    expect(activeState).toBe("active");
    expect(differentState).toBe("anotherState");
  });

  it("after starting a command the active state is true", async () => {
    await startCmd({ db: dbName, field: "field" });
    expect(await getActiveState(db, "field")).toBe(true);
  });

  it("after ending a command the active state is false", async () => {
    await startCmd({ db: dbName, field: "field" });
    const endDoc = await endCmd({ db: dbName, field: "field" });
    expect(endDoc.data.lastState).toBe(true);
    expect(await getActiveState(db, "field")).toBe(false);
  });

  it("it can be switched multiple times", async () => {
    const switchDoc1 = await switchCmd({
      db: dbName,
      field: "field",
      state: "active",
    });
    expect(switchDoc1.data.lastState).toBe(null);
    expect(await getActiveState(db, "field")).toBe("active");

    const switchDoc2 = await switchCmd({
      db: dbName,
      field: "field",
      state: "inactive",
    });
    expect(switchDoc2.data.lastState).toBe("active");
    expect(await getActiveState(db, "field")).toBe("inactive");

    const endDoc = await endCmd({ db: dbName, field: "field" });
    expect(endDoc.data.lastState).toBe("inactive");
    expect(await getActiveState(db, "field")).toBe(false);
  });

  it("the state can be switched to null", async () => {
    await switchCmd({ db: dbName, field: "field", state: "active" });
    await switchCmd({ db: dbName, field: "field", state: null });
    expect(await getActiveState(db, "field")).toBe(null);
  });

  it("can get the state of a field at different points in time", async () => {
    setNow("2023-09-02,7:15");
    await switchCmd({ db: dbName, field: "machine", state: "preparing" });
    await  at(
      "7:20",
      switchCmd
    )({ db: dbName, field: "machine", state: "warm" });
    await at(
      "9",
      switchCmd
    )({ db: dbName, field: "machine", state: "reading" });
    await at(
      "10",
      switchCmd
    )({
      db: dbName,
      time: "910",
      field: "machine",
      state: "thinking",
    });
    await at(
      "11",
      switchCmd
    )({
      db: dbName,
      field: "machine",
      duration: 15,
      state: "distracted",
    });
    await at("12:30", endCmd)({ db: dbName, field: "machine" });
    await at("12:35", startCmd)({ db: dbName, field: "machine" });
    await at(
      "12:45",
      switchCmd
    )({ db: dbName, field: "machine", state: "overheating" });
    await at(
      "18",
      switchCmd
    )({ db: dbName, field: "machine", state: "exploded" });
    await at("21", switchCmd)({ db: dbName, field: "machine", state: null });
    setNow("2023-09-02,22:00");
    expect(await getActiveState(db, "machine")).toBe(null);

    expect(await getActiveState(db, "machine", "7:14")).toBe(null);
    expect(await getActiveState(db, "machine", "7:15")).toBe("preparing");
    expect(await getActiveState(db, "machine", "7:20")).toBe("warm");
    expect(await getActiveState(db, "machine", "8:20")).toBe("warm");
    expect(await getActiveState(db, "machine", "9")).toBe("reading");
    expect(await getActiveState(db, "machine", "915")).toBe("thinking");
    expect(await getActiveState(db, "machine", "1045")).toBe("distracted");
    expect(await getActiveState(db, "machine", "1059")).toBe("distracted");
    expect(await getActiveState(db, "machine", "11")).toBe("thinking");
    expect(await getActiveState(db, "machine", "12:30")).toBe(false);
    expect(await getActiveState(db, "machine", "12:35")).toBe(true);
    expect(await getActiveState(db, "machine", "13")).toBe("overheating");
    expect(await getActiveState(db, "machine", "20")).toBe("exploded");
    expect(await getActiveState(db, "machine", "22")).toBe(null);

    expect(await getActiveState(db, "machine", DateTime.fromISO("10:50"))).toBe(
      "distracted"
    );
  });
});
