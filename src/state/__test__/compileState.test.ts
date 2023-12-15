import { compileState } from "../compileState";
import { BaseArgs } from "../../input/baseArgs";
import * as normalizeStateModule from "../normalizeState";
import * as activeStateModule from "../getActiveState";
import SpyInstance = jest.SpyInstance;
import { toDatumTime } from "../../time/timeUtils";
import { DatumViewMissingError } from "../../errors";

const occurTime = toDatumTime("2023-12-15T09:45:00.000Z");
const args: BaseArgs = {};
describe("compileState", () => {
  let normalizeStateSpy: SpyInstance;
  let activeStateSpy: SpyInstance;
  beforeEach(() => {
    normalizeStateSpy = jest
      .spyOn(normalizeStateModule, "normalizeState")
      .mockReturnValue("normalizedState");
    activeStateSpy = jest
      .spyOn(activeStateModule, "getActiveState")
      .mockResolvedValue("activeState");
  });

  it("normalizes state if it exists", async () => {
    for (const state of ["active", true, null, ["array", "state"]]) {
      const returnedData = await compileState({ state }, args);
      expect(normalizeStateSpy).toHaveBeenCalledWith(state);
      expect(returnedData.state).toEqual("normalizedState");
      normalizeStateSpy.mockClear();
    }
  });

  it("also normalizes lastState", async () => {
    for (const state of [false, null, "last"]) {
      const returnedData = await compileState({ lastState: state }, args);
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

  it("calls and uses the activeState for the lastState when there is state, field, and occurTime, but no lastState", async () => {
    expect(
      await compileState(
        {
          state: "state",
          occurTime,
          field: "field",
        },
        args,
      ),
    ).toEqual({
      state: "normalizedState",
      lastState: "activeState",
      occurTime,
      field: "field",
    });
  });

  it("does not call getActiveState if lastState is present, or field or occurTime is missing", async () => {
    expect(
      await compileState(
        {
          state: "state",
          lastState: "lastState",
          occurTime,
          field: "field",
        },
        args,
      ),
    ).toEqual({
      state: "normalizedState",
      lastState: "normalizedState",
      occurTime,
      field: "field",
    });
    expect(
      await compileState(
        {
          state: "state",
          field: "field",
        },
        args,
      ),
    ).toEqual({
      state: "normalizedState",
      field: "field",
    });
    expect(
      await compileState(
        {
          state: "state",
          occurTime,
        },
        args,
      ),
    ).toEqual({
      state: "normalizedState",
      occurTime,
    });
    expect(activeStateSpy).not.toHaveBeenCalled();
  });

  it("only adds lastState to a payload without state if lastState is null", async () => {
    expect(
      await compileState({ occurTime, field: "field" }, args),
    ).not.toHaveProperty("lastState");
    expect(activeStateSpy).toHaveBeenCalled();
    activeStateSpy.mockClear();

    activeStateSpy.mockResolvedValue(null);
    expect(await compileState({ occurTime, field: "field" }, args)).toEqual({
      lastState: null,
      occurTime,
      field: "field",
    });
    expect(activeStateSpy).toHaveBeenCalledTimes(1);

    activeStateSpy.mockClear();
    // Still only calls if both occurTime and field
    activeStateSpy.mockResolvedValue(null);
    expect(await compileState({ field: "field" }, args)).not.toHaveProperty(
      "lastState",
    );
    expect(await compileState({ occurTime }, args)).not.toHaveProperty(
      "lastState",
    );

    expect(activeStateSpy).not.toHaveBeenCalled();
  });
});
