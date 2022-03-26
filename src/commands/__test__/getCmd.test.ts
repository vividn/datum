import { testDbLifecycle } from "../../test-utils";
import { setupCmd } from "../setupCmd";
import { getCmd } from "../getCmd";
import { Show } from "../../output/output";

const dbName = "get_cmd_test";
const db = testDbLifecycle(dbName);

beforeEach(async () => {
  await setupCmd({ db: dbName });
});

it("gets a document based on the first few letters of humanId", async () => {
  const doc = { _id: "hello", data: {}, meta: { humanId: "a44quickId" } };
  await db.insert(doc);
  const returned = await getCmd({ db: dbName, quickId: "a44" });

  expect(returned).toEqual({ ...doc, _rev: expect.anything() });
});

it("gets a document based on the first few letters of _id", async () => {
  const doc = { _id: "the_quick_brown_fox", foo: "abc" };
  await db.insert(doc);
  const returned = await getCmd({ db: dbName, quickId: "the_qu" });

  expect(returned).toEqual({ ...doc, _rev: expect.anything() });
});

it("outputs an EXISTS message when show is standard", async () => {
  const originalLog = console.log;
  const mockLog = jest.fn();
  console.log = mockLog;

  const doc = {
    _id: "show_me_a_message",
    data: {},
    meta: { humanId: "somethingElse" },
  };
  await db.insert(doc);
  await getCmd({ db: dbName, quickId: "show_me", show: Show.Standard });
  expect(mockLog).toHaveBeenCalledWith(expect.stringContaining("EXISTS"));

  console.log = originalLog;
});
