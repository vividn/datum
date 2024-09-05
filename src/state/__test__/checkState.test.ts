import { restoreNow, setNow, testDbLifecycle } from "../../__test__/test-utils";
import { switchCmd } from "../../commands/switchCmd";
import { DatumDocument } from "../../documentControl/DatumDocument";
import { checkState } from "../checkState";

describe("checkState", () => {
  const db = testDbLifecycle("check_state_test");

  beforeEach(() => {
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
    setNow("+5");
    const doc3 = (await switchCmd(
      "field state3 --last-state state2",
    )) as DatumDocument; // explicit lastState
    expect(doc3.data.lastState).toEqual("state2");

    const retVal = await checkState({ db, field: "field" });
    expect(retVal).toBe(true);
  });
  it.todo(
    "throws an IncorrectLastStateError if lastState does not reflect the last state correctly",
  );
  it.todo(
    "throws an OverlappingBlockError if a state change block is inserted and overlaps another state change",
  );
  it.todo(
    "throws an OverlappingBLockError if a state change is added in the middle of an existing block",
  );
  it.todo("throws an OverlappingBlockError if two blocks overlap an edge");
  it.todo(
    "throws an IncorrectLastStateError if one block is nested in another, but has the wrong lastState",
  );
  it.todo("correctly handles based off of a given startTime");
  it.todo(
    "throws an IncorrectLastStateError if the lastState of the first ever row for a field is not null",
  );
  it.todo(
    "does not throw an error if the first row does not have a null lastState when a startTime is given",
  );
});
