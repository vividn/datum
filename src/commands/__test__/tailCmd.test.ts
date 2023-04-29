import { mockedLogLifecycle, testDbLifecycle } from "../../test-utils";

describe("tailCmd", () => {
  const _mockedLog = mockedLogLifecycle();
  const dbName = "tail_cmd_test";
  const _db = testDbLifecycle(dbName);

  it.todo("displays the last 10 occurrences in the database");
  it.todo("can display the last n occurrences");
  it.todo("can display a custom format for the tail commands");
});
