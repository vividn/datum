import { restoreNow, testDbLifecycle } from "../../test-utils";

describe("getActiveState", () => {
  const dbName = "get_active_state_test";
  testDbLifecycle(dbName);

  afterEach(() => {
    restoreNow();
  });

  it.todo("returns null if no state has ever been recorded");
  it.todo(
    "returns null even if another field has been changed to a state (both before and after alphabetically)"
  );
  it.todo("after switching to a state the active state is that new state");
  it.todo("it can be switched multiple times");
  it.todo("after starting a command the active state is true");
  it.todo("after stopping a command the active state is false");
  it.todo("the state can be switched to null");
  it.todo("can get the state of a field at different points in time");
});
