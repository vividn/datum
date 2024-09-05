import { restoreNow, setNow, testDbLifecycle } from "../../__test__/test-utils";
import { setupCmd } from "../../commands/setupCmd";
import { switchCmd } from "../../commands/switchCmd";
import { DatumDocument } from "../../documentControl/DatumDocument";
import {
  checkState,
  LastStateError,
  OverlappingBlockError,
} from "../checkState";

describe("checkState", () => {
  const db = testDbLifecycle("check_state_test");

  beforeEach(() => {
    setupCmd({});
    setNow("2024-09-05 10:15:00");
  });

  afterEach(() => {
    restoreNow();
  });

  it("returns true for a field with no data", async () => {
    const retVal = await checkState({ db, field: "test" });
    expect(retVal).toBe(true);
  });

  it("returns true if every state transistions properly to the next state", async () => {
    await switchCmd("field state1");
    setNow("+5");
    const doc2 = (await switchCmd("field state2")) as DatumDocument; // automatically adds the lastState
    expect(doc2.data.lastState).toEqual("state1");
    setNow("+10");
    const doc3 = (await switchCmd(
      "field state3 --last-state state2",
    )) as DatumDocument; // explicit lastState
    expect(doc3.data.lastState).toEqual("state2");

    const doc4 = (await switchCmd(
      "field tempState dur=4 -t -4",
    )) as DatumDocument; // puts a block in the middle of state2
    expect(doc4.data.lastState).toEqual("state2");

    const retVal = await checkState({ db, field: "field" });
    expect(retVal).toBe(true);
  });

  it("throws a LastStateError if lastState does not reflect the last state correctly", async () => {
    await switchCmd("field state1");
    setNow("+5");
    await switchCmd("field newState --last-state wrongState");

    await expect(checkState({ db, field: "field" })).rejects.toThrow(
      LastStateError,
    );
  });

  it("throws an OverlappingBlockError if a state change block is inserted and overlaps another state change", async () => {
    setNow("10:45");
    await switchCmd("project emails");
    setNow("11");
    await switchCmd("project meetings");
    setNow("11:10");
    await switchCmd("project overlapping dur=15");

    await expect(checkState({ db, field: "project" })).rejects.toThrow(
      OverlappingBlockError,
    );
  });

  it("throws an OverlappingBLockError if a state change is added in the middle of an existing block", async () => {
    setNow("11");
    await switchCmd("project emails");
    setNow("11:30");
    await switchCmd("project frontend dur=15");
    setNow("11:45");
    await switchCmd("project backend -t 11:20"); // starts in the middle of the frontend block

    await expect(checkState({ db, field: "project" })).rejects.toThrow(
      OverlappingBlockError,
    );
  });

  it("throws an OverlappingBlockError if two blocks overlap an edge", async () => {
    setNow("12");
    await switchCmd("project emails");
    setNow("12:30");
    await switchCmd("project frontend dur=20");
    setNow("12:40");
    await switchCmd("project backend dur=15");

    await expect(checkState({ db, field: "project" })).rejects.toThrow(
      OverlappingBlockError,
    );
  });

  it("throws an OverlappingBlockError if one block is nested in another, and has the wrong lastState", async () => {
    // checkState only notices fully nested blocks as being a problem when the lastState is incorrect. However, overlapping blocks in any form is unsupported and would be caught by checkOverlappingBlocks
    setNow("12");
    await switchCmd("project emails");
    setNow("12:30");
    await switchCmd("project frontend dur=20");
    await switchCmd("project backend -t -5 dur=10 --last-state notFrontend");

    await expect(checkState({ db, field: "project" })).rejects.toThrow(
      OverlappingBlockError,
    );
  });

  it("correctly handles based off of a given startTime and/or endTime", async () => {
    setNow("2024-09-05T13:00:00Z");
    await switchCmd("field state1");
    setNow("+5");
    await switchCmd("field state2");
    setNow("+10");
    await switchCmd("field state3 --last-state wrongState");
    setNow("2024-09-05T14:00:00Z");
    await switchCmd("field state4");
    setNow("+5");
    await switchCmd("field overlappingBlock dur=10");
    setNow("2024-09-05T15:00:00Z");
    await switchCmd("field state5");
    setNow("+10");
    await switchCmd("field state6");

    await expect(
      checkState({ db, field: "field", startTime: "2024-09-05T13:00:00Z" }),
    ).rejects.toThrow(LastStateError);
    await expect(
      checkState({ db, field: "field", startTime: "2024-09-05T13:55:00Z" }),
    ).rejects.toThrow(OverlappingBlockError);
    await expect(
      checkState({
        db,
        field: "field",
        startTime: "2024-09-05T12:00:00Z",
        endTime: "2024-09-05T13:10:00Z",
      }),
    ).resolves.toBe(true);
    // await expect({
    //   db,
    //   field: "field",
    //   endTime: "2024-09-05T13:10:00Z",
    // }).resolves.toBe(true);
    await expect(
      checkState({
        db,
        field: "field",
        startTime: "2024-09-05T14:55:00Z",
      }),
    ).resolves.toBe(true);
  });

  it("throws an IncorrectLastStateError if the lastState of the first ever row for a field is not null", async () => {
    await switchCmd("field newState --last-state false");
    await expect(checkState({ db, field: "field" })).rejects.toThrow(
      LastStateError,
    );

    await switchCmd("field2 newState --last-state null");
    await expect(checkState({ db, field: "field2" })).resolves.toBe(true);
  });

  it("does not throw an error if the first row does not have a null lastState when a startTime is given", async () => {
    setNow("2024-09-05T13:00:00Z");
    await switchCmd("field newState --last-state false");
    await expect(
      checkState({ db, field: "field", startTime: "2024-09-05T12:55:00Z" }),
    ).resolves.toBe(true);
  });
});