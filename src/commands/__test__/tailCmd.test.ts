import { mockedLogLifecycle, setNow, testDbLifecycle } from "../../test-utils";
import { endCmd } from "../endCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";

const today = "2023-10-16";
async function generateSampleDay(date: string) {
  setNow(`8:30 ${date}`);
  await endCmd({ field: "sleep" });
  setNow("+5");
  await startCmd({ field: "sleep", yesterday: 1, time: "23:20" });
  setNow("8:40");
  await switchCmd({ field: "project", state: "german" });
  await switchCmd({ field: "text", state: "fiction_book" });
  setNow("9:10");
  await endCmd({ field: "project" });
  await endCmd({ field: "text" });
  setNow("9:30");
  await switchCmd({ field: "environment", state: "outside" });
  await startCmd({ field: "stretch" });
  setNow("+7");
  await endCmd({ field: "stretch" });
  await startCmd({ field: "run" });
  setNow("+30");
  await endCmd({ field: "run", data: ["distance=5.4"] });
}

describe("tailCmd", () => {
  const _mockedLog = mockedLogLifecycle();
  const dbName = "tail_cmd_test";
  const _db = testDbLifecycle(dbName);

  it.todo("displays by default the last 10 occurrences in the database");
  it.todo("will display all if there are less than 10 occurences in db");
  it.todo("can display the last n occurrences");
  it.todo("displays last occurrences of a specific field");
  it.todo("defaults to a hybrid display of occurTime and createTime");
  it.todo("can just display occurTime tail");
  it.todo("can display modifyTime tail");
  it.todo("can display createTime tail");
  it.todo("can display a tail from a certain moment in time");
  it.todo(
    "displays future entries if no specific time is given or --no-timestamp is given"
  );
  it.todo(
    "does not display future entries if now is given specifically as the time"
  );
  it.todo("displays all occurrences on a day if date is given without time");
  it.todo(
    "displays only n latest occurrences on a day if date and -n is given"
  );

  it.todo("can display a custom format for the tail commands");
});
