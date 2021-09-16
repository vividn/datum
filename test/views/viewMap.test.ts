import { it } from "@jest/globals";
import { mock } from "jest-mock-extended";
import { EitherDocument } from "../../src/documentControl/DatumDocument";
import { DocumentScope, DocumentViewResponse } from "nano";
import { DatumView } from "../../src/views/viewDocument";
import {
  mockDocDeletedError,
  mockDocMissingError,
  mockMissingNamedViewError,
} from "../test-utils";
import { DatumViewMissingError } from "../../src/errors";
import viewMap from "../../src/views/viewMap";

const mockDb = mock<DocumentScope<EitherDocument>>();
const mockDatumView = mock<DatumView>();


beforeEach(() => {
  jest.resetAllMocks();
  mockDatumView.name = "mock_datum_view";
});

it("calls the default view in the named DatumView on the db unreduced", async () => {
  await viewMap({
    db: mockDb,
    datumView: mockDatumView,
    params: { key: "abc" },
  });
  const designDocName = "_design/" + mockDatumView.name;
  expect(mockDb.view).toBeCalledWith(designDocName, "default", {reduce: false, key: "abc"});
});

it("returns the view result directly", async () => {
  const mockViewResponse: DocumentViewResponse<unknown, EitherDocument> = {total_rows: 1, rows: [{key: "abc", value: 37, id: "abc__37"}], offset: 1337, update_seq: undefined};
  mockDb.view.mockReturnValue(Promise.resolve(mockViewResponse));
  const returnValue = await viewMap({db: mockDb, datumView: mockDatumView, params: { key: "abc" }});
  expect(returnValue).toEqual(mockViewResponse);
});

it("throws a DatumViewMissingError if the view document is not there or the named view is missing from the document", async () => {
  mockDb.view
    .mockRejectedValueOnce(mockDocMissingError)
    .mockRejectedValueOnce(mockDocDeletedError)
    .mockRejectedValueOnce(mockMissingNamedViewError)
    .mockRejectedValueOnce(new Error("should not be caught"));

  await expect(() =>
    viewMap({ db: mockDb, datumView: mockDatumView })
  ).rejects.toThrowError(DatumViewMissingError);
  await expect(() =>
    viewMap({ db: mockDb, datumView: mockDatumView })
  ).rejects.toThrowError(DatumViewMissingError);
  await expect(() =>
    viewMap({ db: mockDb, datumView: mockDatumView })
  ).rejects.toThrowError(DatumViewMissingError);
  try {
    await viewMap({ db: mockDb, datumView: mockDatumView });
  } catch (error: any) {
    expect(error).not.toBeInstanceOf(DatumViewMissingError);
    expect(error.message).toEqual("should not be caught");
  }
});
