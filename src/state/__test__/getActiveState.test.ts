import { restoreNow, setNow, testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../../commands/setupCmd";
import { getActiveState } from "../getActiveState";
import { switchCmd } from "../../commands/switchCmd";
import { startCmd } from "../../commands/startCmd";
import { endCmd } from "../../commands/endCmd";
import { DateTime } from "luxon";
import { occurCmd } from "../../commands/occurCmd";

describe("getActiveState", () => {
  const dbName = "get_active_state_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd("");
  });

  afterEach(() => {
    restoreNow();
  });

  it("returns null if no state has ever been recorded", async () => {
    const activeState = await getActiveState(db, "field");
    expect(activeState).toBe(null);
  });

  it("returns null even if another field has been changed to a state (both before and after alphabetically)", async () => {
    await switchCmd("fald active");
    await switchCmd("fold someState");
    const activeState = await getActiveState(db, "field");
    expect(activeState).toBe(null);
  });

  it("after switching to a state the active state is that new state", async () => {
    await switchCmd("field active");
    const activeState = await getActiveState(db, "field");
    expect(activeState).toBe("active");
  });
  it("after switching to a state the active state is that new state even if another field has been changed", async () => {
    await switchCmd("field active");
    await switchCmd("different anotherState");
    const activeState = await getActiveState(db, "field");
    const differentState = await getActiveState(db, "different");
    expect(activeState).toBe("active");
    expect(differentState).toBe("anotherState");
  });

  it("after starting a command the active state is true", async () => {
    await startCmd("field");
    expect(await getActiveState(db, "field")).toBe(true);
  });

  it("after ending a command the active state is false", async () => {
    await startCmd("field");
    const endDoc = await endCmd("field");
    expect(endDoc.data.lastState).toBe(true);
    expect(await getActiveState(db, "field")).toBe(false);
  });

  it("it can be switched multiple times", async () => {
    const switchDoc1 = await switchCmd("field active");
    expect(switchDoc1.data.lastState).toBe(null);
    expect(await getActiveState(db, "field")).toBe("active");

    const switchDoc2 = await switchCmd("field inactive");
    expect(switchDoc2.data.lastState).toBe("active");
    expect(await getActiveState(db, "field")).toBe("inactive");

    const endDoc = await endCmd("field");
    expect(endDoc.data.lastState).toBe("inactive");
    expect(await getActiveState(db, "field")).toBe(false);
  });

  it("the state can be switched to null", async () => {
    await switchCmd("field active");
    await switchCmd("field null");
    expect(await getActiveState(db, "field")).toBe(null);
  });

  it("can get the state of a field at different points in time", async () => {
    setNow("2023-09-02,7:15");
    await switchCmd("machine preparing");

    setNow("7:20");
    await switchCmd("machine warm");

    setNow("9");
    await switchCmd("machine reading");

    setNow("10");
    await switchCmd("machine thinking -t 910");

    setNow("11");
    await switchCmd("machine distracted 15");

    setNow("12:30");
    await endCmd("machine");

    setNow("12:35");
    await startCmd("machine");

    setNow("12:45");
    await switchCmd("machine overheating");

    setNow("18");
    await switchCmd("machine exploded");

    setNow("21");
    await switchCmd("machine null");

    setNow("22");
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

  it("transitions to having a false state if an occurrence or durational occurrence of the field occurs", async () => {
    expect(await getActiveState(db, "field")).toBe(null);
    await switchCmd("field active 5m");
    expect(await getActiveState(db, "field")).toBe(false);

    await switchCmd("field null");
    expect(await getActiveState(db, "field")).toBe(null);
    await occurCmd("field");
    expect(await getActiveState(db, "field")).toBe(false);
  });
});
