import { mockedLogLifecycle, testDbLifecycle } from "../../test-utils";

describe("tailCmd", () => {
  const _mockedLog = mockedLogLifecycle();
  const _db = testDbLifecycle("tail_cmd_test");

  it.todo("displays the last 10 occurences in the database");
  it.todo("can display the last n occurences");
});
