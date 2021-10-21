import { it, jest } from "@jest/globals";
import { mock } from "jest-mock-extended";
import { DocumentScope, DocumentViewResponse } from "nano";
import { idToHumanView } from "../../views/datumViews";
import getHumanIds from "../getHumanIds";
import {
  mockDocDeletedError,
  mockDocMissingError,
  mockMissingNamedViewError,
} from "../../../test/test-utils";
import { DatumViewMissingError } from "../../errors";

const dbMock = mock<DocumentScope<any>>();

beforeEach(() => {
  jest.resetAllMocks();
});

it("calls the humanId view with the input _ids as keys, then calls minHid with the returned humanIds", async () => {
  const viewName = idToHumanView.name;
  const idsHids = [
    ["id1", "hid1"],
    ["id2", "hid2"],
    ["id3", "hid3"],
  ];
  const mockViewResult: DocumentViewResponse<string, any> = {
    total_rows: 1000,
    offset: 300,
    rows: idsHids.map((idHid) => ({
      id: idHid[0],
      key: idHid[0],
      value: idHid[1],
    })),
    update_seq: undefined,
  };
  dbMock.view.mockReturnValue(Promise.resolve(mockViewResult));

  const inputIds = ["id1", "id2", "id_with_no_humanId", "id3"];
  const returnVal = await getHumanIds(dbMock, inputIds);

  expect(dbMock.view).toHaveBeenCalledTimes(1);
  expect(dbMock.view).toBeCalledWith(
    viewName,
    "default",
    expect.objectContaining({ reduce: false, keys: inputIds })
  );

  expect(returnVal).toEqual(["hid1", "hid2", undefined, "hid3"]);
});

it("throws a DatumViewMissing if datum view is not present", async () => {
  dbMock.view
    .mockRejectedValueOnce(mockDocMissingError)
    .mockRejectedValueOnce(mockDocDeletedError)
    .mockRejectedValueOnce(mockMissingNamedViewError)
    .mockRejectedValueOnce(new Error("should not be caught"));

  await expect(() => getHumanIds(dbMock, ["abc"])).rejects.toThrowError(
    DatumViewMissingError
  );
  await expect(() => getHumanIds(dbMock, ["abc"])).rejects.toThrowError(
    DatumViewMissingError
  );
  await expect(() => getHumanIds(dbMock, ["abc"])).rejects.toThrowError(
    DatumViewMissingError
  );
  try {
    await getHumanIds(dbMock, ["abc"]);
  } catch (error: any) {
    expect(error).not.toBeInstanceOf(DatumViewMissingError);
    expect(error.message).toEqual("should not be caught");
  }
});
