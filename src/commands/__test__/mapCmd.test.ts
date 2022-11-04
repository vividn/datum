import { mockedLogLifecycle, testDbLifecycle } from "../../test-utils";

describe("mapCmd", () => {
  const dbName = "map_cmd_test";
  const db = testDbLifecycle(dbName);
  const mockedLog = mockedLogLifecycle();

  it.todo("displays all the rows of the default map function");
  it.todo("still displays the map table even if a reduce function is specified");
  it.todo("can specify a different map function using /notation");
  it.todo("prioritizes a design document with / in it over using the named function of whats prior to the slash");
  it.todo("can display a different map function by specifying view");
});
