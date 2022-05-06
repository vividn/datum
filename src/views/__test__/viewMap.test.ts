import { mock } from "jest-mock-extended";
import {
  EitherDocument,
  EitherPayload,
} from "../../documentControl/DatumDocument";
import { DocumentScope, DocumentViewResponse } from "nano";
import { DatumView, StringifiedDatumView } from "../viewDocument";
import {
  mockDocDeletedError,
  mockDocMissingError,
  mockMissingNamedViewError,
  pass,
  testNano,
} from "../../test-utils";
import { DatumViewMissingError } from "../../errors";
import { viewMap } from "../viewMap";
import { insertDatumView } from "../insertDatumView";

const mockDb = mock<DocumentScope<EitherDocument>>();
const mockDatumView = mock<DatumView>();

beforeEach(() => {
  mockDatumView.name = "mock_datum_view";
});

it("calls the default view in the named DatumView on the db unreduced", async () => {
  await viewMap({
    db: mockDb,
    datumView: mockDatumView,
    params: { key: "abc" },
  });
  expect(mockDb.view).toBeCalledWith(mockDatumView.name, "default", {
    reduce: false,
    key: "abc",
  });
});

it("returns the view result directly", async () => {
  const mockViewResponse: DocumentViewResponse<unknown, EitherDocument> = {
    total_rows: 1,
    rows: [{ key: "abc", value: 37, id: "abc__37" }],
    offset: 1337,
    update_seq: undefined,
  };
  mockDb.view.mockReturnValue(Promise.resolve(mockViewResponse));
  const returnValue = await viewMap({
    db: mockDb,
    datumView: mockDatumView,
    params: { key: "abc" },
  });
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

it("can call a view that has been inserted by insertDatumView", async () => {
  const dbName = "test_view_map";
  await testNano.db.destroy(dbName).catch(pass);
  await testNano.db.create(dbName);
  const db = testNano.use<EitherPayload>(dbName);

  const testDatumView: StringifiedDatumView = {
    name: "datum_test_view_datum_view",
    map: `(doc) => {emit(doc._id, null)}`,
  };
  await insertDatumView({ db, datumView: testDatumView });
  await expect(viewMap({ db, datumView: testDatumView })).resolves.toBeTruthy();
});
