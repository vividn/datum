import { DocumentViewResponse } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";

export type WithRequired<T, K extends keyof T> = T & { [P in K]-?: T[P] };

export type ViewRow<RowValueType> = DocumentViewResponse<
  RowValueType,
  EitherPayload
>["rows"][0];
