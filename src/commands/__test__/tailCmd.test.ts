import {
  mockedLogLifecycle,
  popNow,
  pushNow,
  setNow,
  testDbLifecycle,
} from "../../test-utils";
import { endCmd } from "../endCmd";
import { startCmd } from "../startCmd";
import { switchCmd } from "../switchCmd";
import { occurCmd } from "../occurCmd";
import { tailCmd } from "../tailCmd";
import { Show } from "../../input/outputArgs";
import { setupCmd } from "../setupCmd";
import { addCmd } from "../addCmd";
import { updateCmd } from "../updateCmd";

const yesterday = "2023-10-15";
const today = "2023-10-16";
const tomorrow = "2023-10-17";

async function generateSampleMorning(date: string) {
  pushNow(`8:30 ${date}`);
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
  await startCmd({ field: "stretch" });
  setNow("+8");
  await endCmd({ field: "stretch" });
  await switchCmd({ field: "environment", state: "home" });
  setNow("+3");
  await occurCmd({ field: "pushup", data: ["amount=10"] });
  setNow("11");
  await occurCmd({
    field: "caffeine",
    data: ["amount=100"],
    comment: "coffee",
  });
  popNow();
}

describe("tailCmd", () => {
  const mockedLog = mockedLogLifecycle();
  const dbName = "tail_cmd_test";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await setupCmd({});
    setNow(`20:00 ${today}`);
  });

  it("displays by default the last 10 occurrences in the database", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd({ show: Show.Standard });
    expect(docs.length).toBe(10);
    expect(docs[0]._id).toMatchInlineSnapshot(
      `"environment:2023-10-16T09:30:00.000Z"`
    );
    expect(docs.at(-1)?._id).toMatchInlineSnapshot(
      `"caffeine:2023-10-16T11:00:00.000Z"`
    );
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it("will display all if there are less than 10 occurences in db", async () => {
    setNow(`8am ${today}`);
    await occurCmd({ field: "caffeine", data: ["amount=100"] });
    setNow(`10am`);
    await switchCmd({ field: "project", state: "household" });
    setNow(`10:30`);
    await endCmd({ field: "project" });

    const docs = await tailCmd({ show: Show.Standard });
    expect(docs.length).toBe(3);
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it("can display the last n occurrences", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd({ n: 5 });
    expect(docs.length).toBe(5);
  });

  it("will display all if there are less than n occurences in db", async () => {
    setNow(`8am ${today}`);
    await occurCmd({ field: "caffeine", data: ["amount=100"] });
    setNow(`10am`);
    await switchCmd({ field: "project", state: "household" });
    setNow(`10:30`);
    await endCmd({ field: "project" });

    const docs = await tailCmd({ n: 5 });
    expect(docs.length).toBe(3);
  });

  it("displays last occurrences of a specific field", async () => {
    await generateSampleMorning(today);
    const docs = await tailCmd({ field: "stretch" });
    expect(docs.length).toBe(4);
    await generateSampleMorning(yesterday);
    const docs2 = await tailCmd({ field: "stretch" });
    expect(docs2.length).toBe(8);
    expect(docs2.map((doc) => doc.data.field)).toEqual(
      Array(8).fill("stretch")
    );
  });

  it("defaults to a hybrid display of occurTime and createTime", async () => {
    setNow("19:00");
    const { _id: occur1Id } = await occurCmd({ field: "alcohol" });
    setNow("20:00");
    const { _id: addId } = await addCmd({
      field: "person",
      baseData: { name: "john doe", age: 35 },
      idPart: "%name",
    });
    setNow("21:00");
    const { _id: occur2Id } = await startCmd({
      field: "socialize",
      time: "19:30",
    });
    const { _id: occur3Id } = await endCmd({ field: "socialize" });

    // modification does not change the placement in tail, since hybrid is based off of creation when there is no occurence
    setNow("22:00");
    await updateCmd({ quickId: addId, data: ["age=36"] });

    const docs = await tailCmd({ show: Show.Standard });
    expect(docs.length).toBe(4);
    expect(docs.map((doc) => doc._id)).toEqual([
      occur1Id,
      occur2Id,
      addId,
      occur3Id,
    ]);
    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

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

describe("headCmd", () => {});
