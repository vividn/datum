import { mockedLogLifecycle, testDbLifecycle } from "../../test-utils";

describe("tailCmd", () => {
  const _mockedLog = mockedLogLifecycle();
  const dbName = "tail_cmd_test";
  const _db = testDbLifecycle(dbName);

  it.todo("displays the last 10 occurrences in the database");
  it.todo("can display the last n occurrences");
  it.todo("displays last occurrences of a specific field");
  it.todo("defaults to a hybrid display of occurTime and modifyTime");
  it.todo("can just display occurTime tail");
  it.todo("can display modifyTime tail");
  it.todo("can display createTime tail");
  it.todo("can display a tail from a certain moment in time");
  it.todo("displays future entries if no specific time is given or --no-timestamp is given");
  it.todo("does not display future entries if now is given specifically as the time");
  it.todo("displays all occurrences on a day if date is given without time");
  it.todo(
    "displays only n latest occurrences on a day if date and -n is given"
  );

  it.todo("can display a custom format for the tail commands");
});
