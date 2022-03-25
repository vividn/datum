import {
  mockedLogLifecycle,
  testDbLifecycle,
} from "../../test-utils";

const originalLog = console.log;

describe("tailCmd", () => {
  const mockedLog = mockedLogLifecycle();
  const db = testDbLifecycle("tail_cmd_test");

  it.todo("displays the last 10 occurences in the database");
  it.todo("can display the last n occurences");
});
