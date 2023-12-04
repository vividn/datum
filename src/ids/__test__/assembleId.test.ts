import { assembleId } from "../assembleId";
import { IdError } from "../../errors";

describe("assembleId", () => {
  it("uses the _id in the payload if no idStructure is provided or found in metadata", () => {
    expect(assembleId({ payload: { _id: "dataOnlyId", foo: "bar" } })).toEqual(
      "dataOnlyId",
    );
    expect(
      assembleId({
        payload: {
          _id: "datumId",
          data: { foo: "bar" },
          meta: { humanId: "does-not-have-id-Structure" },
        },
      }),
    );
  });
  it("throws error if no idStructure provided or found, and no _id is in payload", () => {
    expect(() =>
      assembleId({
        payload: {
          data: { abc: "123" },
          meta: { modifyTime: "2020-11-09T00:40:12.544Z" },
        },
      }),
    ).toThrowError(IdError);
    expect(() => assembleId({ payload: { abc: "123" } })).toThrowError(IdError);
  });

  it.todo("doesn't allow recursive %?idStructure% as a part of the id");
});
