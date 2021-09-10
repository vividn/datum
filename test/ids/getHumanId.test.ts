import { it, jest } from "@jest/globals";

const dbMock = jest.fn();

it.todo("calls the humanId view with the input _ids as keys");
it.todo("throws if it receives more than one humanId for some _id");
it.todo(
  "returns an array of the humanIds, with undefined holes for docs that have no row in the view"
);
