import { mockedLogLifecycle, testDbLifecycle } from "../../test-utils";

describe("mapCmd", () => {
  const dbName = "map_cmd_test";
  const db = testDbLifecycle(dbName);
  const mockedLog = mockedLogLifecycle();

  it.todo("displays all the rows of the map function");
});
