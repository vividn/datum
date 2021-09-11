import { it, jest } from "@jest/globals";
import { mock } from "jest-mock-extended";
import { DocumentScope, DocumentViewResponse } from "nano";
import { humanIdView } from "../../src/views/datumViews";
import { getHumanIds } from "../../src/ids/getHumanIds";

const dbMock = mock<DocumentScope<any>>();

beforeEach(() => {
  jest.resetAllMocks();
});

it("calls the humanId view with the input _ids as keys, then calls minHid with the returned humanIds", async () => {
  const viewName = humanIdView.name;
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

  const returnVal = await getHumanIds(dbMock, [
    "id1",
    "id2",
    "id_with_no_humanId",
    "id3",
  ]);

  expect(dbMock.view).toHaveBeenCalledTimes(1);
  expect(dbMock.view).toBeCalledWith(
    viewName,
    "default",
    expect.objectContaining({ reduce: false })
  );

  expect(returnVal).toEqual(["hid1", "hid2", "hid3"]);
});
