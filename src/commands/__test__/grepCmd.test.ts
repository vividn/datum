import {
  coloredChalk,
  mockedLogLifecycle,
  mockSpecs,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { grepCmd } from "../grepCmd";
import { Show } from "../../input/outputArgs";

describe("grepCmd", () => {
  coloredChalk();
  const dbName = "grep_cmd_test";
  const db = testDbLifecycle(dbName);
  const { mockedLog } = mockedLogLifecycle();
  mockSpecs();

  it("finds all the docs that contain the given string", async () => {
    const doc1 = { _id: "doc1", foo: "bar" };
    const doc2 = { _id: "doc2", data: { foo: "baz" } };
    const doc3 = { _id: "doc3", data: { foo: "bbarr" } };
    const doc4 = { _id: "doc4", data: { foo: "qux" } };
    const doc5 = { _id: "doc5", data: { bar: "foo" } };
    const doc6 = {
      _id: "doc6",
      data: { abc: "def" },
      meta: { humanId: "bar" },
    };
    await db.bulkDocs([doc1, doc2, doc3, doc4, doc5, doc6]);

    const retDocs = await grepCmd("bar", { show: Show.Standard });
    expect(retDocs).toHaveLength(4);
    expect(retDocs).toEqual(
      expect.arrayContaining([
        expect.objectContaining(doc1),
        expect.objectContaining(doc3),
        expect.objectContaining(doc5),
        expect.objectContaining(doc6),
      ]),
    );

    expect(mockedLog.mock.calls).toMatchSnapshot();
  });

  it.todo("can exclude docs with match an exclude pattern");
  it.todo("can match based on matching multiple regex patterns");
  it.todo("can match based on any given regex pattern with the --or option ");
});
