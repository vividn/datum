import { mockedLogLifecycle, testDbLifecycle } from "../../test-utils";

describe("tailCmd", () => {
  const _mockedLog = mockedLogLifecycle();
  const dbName = "tail_cmd_test";
  const db = testDbLifecycle(dbName);

  it.todo("displays the last 10 occurences in the database");
  it.todo("can display the last n occurences");
  it.todo("can display a custom format for the tail commands");
});
