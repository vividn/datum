import { compileState } from "../compileState";
import { BaseArgs } from "../../input/baseArgs";
import * as normalizeStateModule from "../normalizeState";

const args: BaseArgs = {};
describe("compileState", () => {
  it("normalizes state if it exists", async () => {
    const normalizeStateSpy = jest
      .spyOn(normalizeStateModule, "normalizeState")
      .mockReturnValue("normalizedState");
    for (const state of ["active", true, null, ["array", "state"]]) {
      const returnedData = await compileState({ state }, args);
      expect(normalizeStateSpy).toHaveBeenCalledWith(state);
      expect(returnedData.state).toEqual("normalizedState");
      normalizeStateSpy.mockClear();
    }
  });

  it("also normalizes lastState", async () => {
    const normalizeStateSpy = jest
      .spyOn(normalizeStateModule, "normalizeState")
      .mockReturnValue("normalizedState");
    for (const state of [false, null, "last"]) {
      const returnedData = await compileState({ lastState }, args);
      expect(normalizeStateSpy).toHaveBeenCalledWith(state);
      expect(returnedData.state).toEqual("normalizedState");
      normalizeStateSpy.mockClear();
    }
    const bothStateAndLast = await compileState(
      { state: "active", lastState: "inactive" },
      args,
    );
    expect(normalizeStateSpy).toHaveBeenCalledTimes(2);
    expect(bothStateAndLast).toMatchObject({
      state: "normalizedState",
      lastState: "normalizedState",
    });
  });

  it("does nothing if there is no state and no occurTime", async () => {});

  it.todo(
    "leaves lastState as it is in the payload even if the real lastState is different",
  );
  it.todo(
    "adds lastState to the payload if is not there and there is state and occurTime",
  );
  it.todo(
    "adds lastState to a payload with occurTime and no state only if lastState is non false",
  );
  it.todo(
    "gives a warning if trying to add lastState from context and the db is not setup. After it, returns without lastState",
  );
});
